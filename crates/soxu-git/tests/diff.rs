use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use soxu_git::{
    DiffBaseline, DiffError, DiffFileMode, DiffKind, DiffOutput, DiffRequest, DiffResult,
    DiffReview, GitError, GitProbe,
};

struct Fixture {
    git: GitProbe,
    temp: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new(args: &[&str]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let git = GitProbe::discover().expect("本机 Git");
        let temp = std::env::temp_dir().join(format!(
            "soxu-readonly-diff-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&temp).expect("独立临时根");
        let temp = temp.canonicalize().expect("规范化临时根");
        let root = temp.join("repo");
        fs::create_dir(&root).expect("夹具工作树");
        let fixture = Self { git, temp, root };
        let mut init = vec!["init", "--quiet", "--template=", "--initial-branch=main"];
        init.extend(args);
        fixture.git(init);
        fixture
    }

    fn command(&self, root: &Path) -> Command {
        self.command_with(root, &[])
    }

    /// 夹具 Git 命令的唯一构造点：清除宿主 `GIT_*`、隔离系统与全局配置。
    ///
    /// `maintenance.auto=false` 与 `gc.auto=0` 与生产只读入口（`probe.rs`、
    /// `checkpoint/git.rs`、`repository_create/sandbox.rs`）的守卫保持一致：夹具提交
    /// 若不禁用后台维护，`git commit` 会派生 detached 的 `git maintenance run --auto`，
    /// 它在 `.git/objects` 下创建并删除 `maintenance.lock`、推进该目录 mtime，
    /// 与整树只读快照断言竞态。`extra_args` 追加在守卫之后，供回归用例显式反向启用。
    fn command_with(&self, root: &Path, extra_args: &[&str]) -> Command {
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
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .args([
                "-c",
                "user.name=Diff Fixture",
                "-c",
                "user.email=diff@example.invalid",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ]);
        command.args(extra_args);
        command
    }

    fn git(&self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Vec<u8> {
        success(
            self.command(&self.root)
                .args(args)
                .output()
                .expect("执行夹具 Git"),
        )
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.root.join(path), bytes).expect("写入夹具");
    }

    fn commit(&self) {
        self.commit_with(&[], "fixture", None)
    }

    /// 与 `commit` 走同一命令形状，但允许回归用例追加 `-c` 覆盖（如重新打开后台维护），
    /// 并在需要时用 trace2 记录本次提交派生的子进程。
    fn commit_with(&self, extra_args: &[&str], label: &str, trace: Option<&Path>) {
        self.git(["add", "--all"]);
        success(
            self.commit_command(extra_args, label, trace)
                .output()
                .expect("执行夹具提交"),
        );
    }

    /// 提交流程的唯一命令构造点：`commit` 与有界回归用例共用，保证两边真的走同一条路径。
    fn commit_command(&self, extra_args: &[&str], label: &str, trace: Option<&Path>) -> Command {
        static COMMIT_SEQ: AtomicU64 = AtomicU64::new(0);
        // 并行测试中的独立仓库也可能提交相同内容，为每次提交分配不同的时间戳。
        let seq = COMMIT_SEQ.fetch_add(1, Ordering::Relaxed);
        let date = format!("{} +0000", 1_700_000_000 + seq);
        let mut command = self.command_with(&self.root, extra_args);
        if let Some(trace) = trace {
            command.env("GIT_TRACE2_EVENT", trace);
        }
        command
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .args(["commit", "-qm", label]);
        command
    }

    async fn diff(
        &self,
        baseline: DiffBaseline<'_>,
        paths: &[&[u8]],
        output: DiffOutput<'_>,
    ) -> Result<DiffResult, DiffError> {
        self.git
            .readonly_diff(
                &self.root,
                DiffRequest {
                    baseline,
                    paths,
                    output,
                },
            )
            .await
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.temp.parent() == std::env::temp_dir().canonicalize().ok().as_deref()
            && self
                .temp
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("soxu-readonly-diff-"))
        {
            let _ = fs::remove_dir_all(&self.temp);
        }
    }
}

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

type Snapshot = BTreeMap<PathBuf, (SystemTime, Option<Vec<u8>>)>;

fn snapshot(root: &Path) -> Snapshot {
    fn walk(root: &Path, paths: &mut Snapshot) {
        let metadata = root.symlink_metadata().expect("夹具元数据");
        assert!(!metadata.is_symlink());
        let contents = metadata
            .is_file()
            .then(|| fs::read(root).expect("夹具字节"));
        paths.insert(
            root.to_owned(),
            (metadata.modified().expect("夹具 mtime"), contents),
        );
        if metadata.is_dir() {
            for child in fs::read_dir(root).expect("遍历夹具") {
                walk(&child.expect("夹具目录条目").path(), paths);
            }
        }
    }
    let mut paths = BTreeMap::new();
    walk(root, &mut paths);
    paths
}

fn text(result: DiffResult) -> Vec<u8> {
    match result.review {
        Some(DiffReview::Text(bytes)) => bytes,
        other => panic!("预期文本审阅，收到 {other:?}"),
    }
}

#[tokio::test]
async fn four_baselines_use_pinned_commits_and_preserve_the_entire_fixture() {
    let fixture = Fixture::new(&[]);
    let path = "- 中文 name.txt";
    let bytes = path.as_bytes();
    fixture.write(path, b"base\n");
    fixture.write("base-only.txt", b"deleted\n");
    fixture.commit();
    fixture.git(["tag", "-am", "fixture", "base"]);
    fixture.write(path, b"head\n");
    fixture.git(["rm", "-q", "base-only.txt"]);
    fixture.commit();
    fixture.write(path, b"index\n");
    fixture.git(["add", "--", path]);
    fixture.write(path, b"worktree\n");
    fixture.write("untracked.txt", b"unselected\n");
    let base = DiffBaseline::BaseToWorktree("refs/tags/base");
    let before = snapshot(&fixture.temp);
    for (mode, old, new) in [
        (DiffBaseline::Unstaged, "-index\n", "+worktree\n"),
        (DiffBaseline::Staged, "-head\n", "+index\n"),
        (DiffBaseline::Uncommitted, "-head\n", "+worktree\n"),
        (base.clone(), "-base\n", "+worktree\n"),
    ] {
        let result = fixture
            .diff(mode, &[bytes], DiffOutput::Review(bytes))
            .await
            .expect("四基准查询");
        assert_eq!(result.changes.len(), 1);
        assert_eq!(result.changes[0].path, bytes);
        let patch = String::from_utf8(text(result)).expect("文本夹具");
        assert!(patch.contains(old) && patch.contains(new), "{patch}");
        assert_eq!(snapshot(&fixture.temp), before);
    }
    let result = fixture
        .diff(base, &[b"base-only.txt"], DiffOutput::Metadata)
        .await
        .expect("仅基准树存在");
    assert_eq!(result.changes[0].kind, DiffKind::Deleted);
    let pinned = result.observation.expect("端点观察");
    let expected = fixture.git(["rev-parse", "refs/tags/base^{commit}"]);
    assert_eq!(
        pinned.resolved_commit_oid.as_deref().map(str::as_bytes),
        expected.strip_suffix(b"\n")
    );
    assert_eq!(pinned.requested_base_ref.as_deref(), Some("refs/tags/base"));
    assert_eq!(snapshot(&fixture.temp), before);
}

#[tokio::test]
async fn unborn_staged_unstaged_and_reverse_changes() {
    let fixture = Fixture::new(&[]);
    fixture.write("file", b"index\n");
    fixture.git(["add", "file"]);
    fixture.write("file", b"work\n");
    assert!(matches!(
        fixture
            .diff(DiffBaseline::Uncommitted, &[b"file"], DiffOutput::Metadata)
            .await,
        Err(DiffError::UnbornHead)
    ));
    let staged = fixture
        .diff(DiffBaseline::Staged, &[b"file"], DiffOutput::Metadata)
        .await
        .expect("unborn staged");
    assert_eq!(staged.changes[0].kind, DiffKind::Added);
    assert!(
        staged
            .observation
            .expect("观察")
            .resolved_commit_oid
            .is_none()
    );
    let unstaged = fixture
        .diff(
            DiffBaseline::Unstaged,
            &[b"file"],
            DiffOutput::Review(b"file"),
        )
        .await
        .expect("unborn unstaged");
    assert!(text(unstaged).ends_with(b"-index\n+work\n"));
    fixture.commit();
    fixture.write("file", b"new index\n");
    fixture.git(["add", "file"]);
    fixture.write("file", b"work\n");
    let net = fixture
        .diff(
            DiffBaseline::Uncommitted,
            &[b"file"],
            DiffOutput::Review(b"file"),
        )
        .await
        .expect("反向抵消");
    assert!(net.changes.is_empty());
    assert_eq!(net.review, Some(DiffReview::Unchanged));
}

#[tokio::test]
async fn linked_detached_uses_its_own_index_and_preserves_shared_metadata() {
    let fixture = Fixture::new(&[]);
    fixture.write("file", b"head\n");
    fixture.commit();
    fixture.write("file", b"main index\n");
    fixture.git(["add", "file"]);
    let linked = fixture.temp.join("linked");
    fixture.git(["worktree", "add", "-q", "--detach", "../linked"]);
    fs::write(linked.join("file"), b"linked index\n").expect("写入 linked 夹具");
    success(
        fixture
            .command(&linked)
            .args(["add", "file"])
            .output()
            .expect("暂存 linked"),
    );
    let before = snapshot(&fixture.temp);
    let result = fixture
        .git
        .readonly_diff(
            &linked,
            DiffRequest {
                baseline: DiffBaseline::Staged,
                paths: &[b"file"],
                output: DiffOutput::Review(b"file"),
            },
        )
        .await
        .expect("linked staged");
    let identity = &result.observation.as_ref().expect("观察").worktree;
    assert_eq!(identity.root, linked);
    assert_eq!(identity.common_dir, fixture.root.join(".git"));
    assert_ne!(identity.git_dir, identity.common_dir);
    assert!(text(result).ends_with(b"-head\n+linked index\n"));
    assert_eq!(snapshot(&fixture.temp), before);
}

#[tokio::test]
async fn refuses_directories_unauthorized_paths_and_unsupported_revisions() {
    let fixture = Fixture::new(&[]);
    fs::create_dir(fixture.root.join("nested")).expect("创建目录夹具");
    fixture.write("nested/file", b"tracked\n");
    fixture.commit();
    fixture.write("nested/file", b"changed\n");
    fixture.write("untracked", b"private\n");
    let empty = fixture
        .git
        .readonly_diff(
            "missing-root",
            DiffRequest {
                baseline: DiffBaseline::BaseToWorktree("invalid"),
                paths: &[],
                output: DiffOutput::Metadata,
            },
        )
        .await
        .expect("空授权不解析路径或 ref");
    assert_eq!(empty, DiffResult::default());
    for path in [b"nested".as_slice(), b"untracked", b"missing"] {
        assert!(matches!(
            fixture
                .diff(DiffBaseline::Unstaged, &[path], DiffOutput::Metadata)
                .await,
            Err(DiffError::UntrackedPath)
        ));
    }
    for path in [
        b"".as_slice(),
        b"../file",
        b"/file",
        b"folder//file",
        b"folder/./file",
        b"C:/file",
        b"file:stream",
        b".git/HEAD",
        b"foo\0bar",
        b":(glob)*",
    ] {
        assert!(matches!(
            fixture
                .diff(DiffBaseline::Unstaged, &[path], DiffOutput::Metadata)
                .await,
            Err(DiffError::InvalidPath)
        ));
    }
    assert!(matches!(
        fixture
            .diff(
                DiffBaseline::Unstaged,
                &[b"nested/file"],
                DiffOutput::Review(b"untracked")
            )
            .await,
        Err(DiffError::UnauthorizedReview)
    ));
    for reference in [
        "main",
        "refs/heads/main~1",
        "refs/heads/main..main",
        "refs/heads/main@{0}",
        "refs/heads/main:path",
        "refs/heads/main^{commit}",
    ] {
        assert!(
            matches!(
                fixture
                    .diff(
                        DiffBaseline::BaseToWorktree(reference),
                        &[b"nested/file"],
                        DiffOutput::Metadata
                    )
                    .await,
                Err(DiffError::InvalidBaseRef)
            ),
            "{reference}"
        );
    }
    let original = fixture.git(["rev-parse", "HEAD"]);
    let oid = std::str::from_utf8(&original).expect("ASCII OID").trim();
    let result = fixture
        .diff(
            DiffBaseline::BaseToWorktree(oid),
            &[b"nested/file"],
            DiffOutput::Metadata,
        )
        .await
        .expect("完整 OID");
    assert_eq!(result.changes[0].old_mode, Some(DiffFileMode::Regular));
    assert!(matches!(
        fixture
            .git
            .readonly_diff(
                &fixture.temp,
                DiffRequest {
                    baseline: DiffBaseline::Unstaged,
                    paths: &[b"file"],
                    output: DiffOutput::Metadata
                }
            )
            .await,
        Err(DiffError::InvalidWorktree)
    ));
}

#[cfg(windows)]
#[tokio::test]
async fn an_unselected_file_can_be_exclusive_locked_without_blocking_diff() {
    use std::os::windows::fs::OpenOptionsExt;

    let fixture = Fixture::new(&[]);
    fixture.write("selected", b"old\n");
    fixture.write("private", b"old\n");
    fixture.commit();
    fixture.write("selected", b"new selected\n");
    fixture.write("private", b"new private\n");
    let private = fixture.root.join("private");
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&private)
        .expect("独占锁住未授权夹具");
    assert!(fs::read(&private).is_err(), "确认夹具正文无法另开读取");
    let result = fixture
        .diff(
            DiffBaseline::Uncommitted,
            &[b"selected"],
            DiffOutput::Review(b"selected"),
        )
        .await
        .expect("未触及独占正文");
    assert_eq!(result.changes.len(), 1);
    assert!(text(result).ends_with(b"-old\n+new selected\n"));
    assert!(
        fixture
            .diff(
                DiffBaseline::Uncommitted,
                &[b"private"],
                DiffOutput::Metadata
            )
            .await
            .is_err()
    );
    drop(locked);
}

#[tokio::test]
async fn binary_and_non_utf8_text_keep_distinct_output_contracts() {
    let fixture = Fixture::new(&[]);
    fixture.write("binary", b"a\0b");
    fixture.write("text", b"old\xff\r\n");
    fixture.write(".gitattributes", b"text text eol=lf\n");
    fixture.commit();
    fixture.write("binary", b"c\0d");
    fixture.write("text", b"new\xff\r\n");
    let before = snapshot(&fixture.temp);
    let binary = fixture
        .diff(
            DiffBaseline::Uncommitted,
            &[b"binary", b"text"],
            DiffOutput::Review(b"binary"),
        )
        .await
        .expect("二进制只返回元数据");
    assert_eq!(binary.review, Some(DiffReview::Binary));
    assert!(binary.changes[0].binary);
    assert!(!binary.changes[1].binary);
    let patch = fixture
        .diff(
            DiffBaseline::Uncommitted,
            &[b"text"],
            DiffOutput::Review(b"text"),
        )
        .await
        .expect("非 UTF-8 文本审阅");
    assert!(text(patch).ends_with(b"-old\xff\n+new\xff\n"));
    assert_eq!(snapshot(&fixture.temp), before);
}

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
/// `.git/objects/maintenance.lock` 由 `git maintenance run` 持有；它的增删就是平台失败的
/// 真实写源，所以这里按 trace2 事件断言子进程的启动与退出。启动与退出都必须绑定
/// **同一发出进程 `sid` + 同一 `child_id`**：只看数字会把后一层 Git 进程的同号子进程
/// 误算成顶层维护进程已结束。
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

/// 走与 `commit` 完全相同的命令形状做一次提交，但把等待做成有界的。
///
/// stdin 关闭、stdout/stderr 不接任何管道：证据只从 trace2 文件读，
/// 因此即便有无关子进程继承了写端，收集端也不会被拖成无界阻塞。
fn traced_commit(fixture: &Fixture, extra_args: &[&str], label: &str) -> CommitObservation {
    static TRACE_SEQ: AtomicU64 = AtomicU64::new(0);
    let trace = std::env::temp_dir().join(format!(
        "soxu-readonly-diff-trace-{}-{}.json",
        std::process::id(),
        TRACE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_file(&trace);
    // 每次提交写入不同内容，确保 `git commit` 不会因 nothing-to-commit 而失败。
    fixture.write("traced", label.as_bytes());
    fixture.git(["add", "--all"]);
    let mut command = fixture.commit_command(extra_args, label, Some(&trace));
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
        "{stage}未在期限内成功（{}）：{observed:?}",
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
    assert!(nested.started, "{nested:?}");
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
/// run 45914578287 上 `binary_and_non_utf8_text_keep_distinct_output_contracts` 的整树快照
/// 只差 `.git/objects/maintenance.lock` 的增删与 `.git/objects` 的 mtime，根因是夹具自身的
/// `git commit` 派生了 detached `git maintenance run --auto`，与快照断言并发写。
/// 本用例不靠重跑或 sleep，而是用 trace2 直接证明受测路径不派生该子进程。
///
/// 正向对照真实走同一条提交路径，只把后台维护重新打开，并要求同步不 detach：
/// 必须同时看到子进程启动、无 `--detach`、以及同 `sid` + 同 `child_id` 的 `child_exit`。
/// 两个提交都走有界等待，阻塞会被判成 `TimedOut` / `WaitErrored` / `ReapUnconfirmed`，
/// 不会拖到 job 超时。
#[test]
fn fixture_commits_never_start_background_maintenance() {
    let guarded = Fixture::new(&[]);
    let observed = traced_commit(&guarded, &[], "guarded\n");
    assert_commit_succeeded(&observed, "受测提交");
    assert!(
        observed.trace.written,
        "trace2 未写入，本次观察无效：{observed:?}"
    );
    assert!(
        !observed.trace.started,
        "夹具提交派生了后台 git maintenance 子进程：{observed:?}"
    );

    let control = Fixture::new(&[]);
    let observed = traced_commit(&control, &CONTROL_MAINTENANCE_ARGS, "control\n");
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

#[tokio::test]
async fn configuration_guards_refuse_filters_includes_and_partial_clone_without_writes() {
    let fixture = Fixture::new(&[]);
    fixture.write("file", b"old\n");
    fixture.commit();
    fixture.write("file", b"new\n");
    fixture.git(["config", "diff.external", "soxu-fixture-unavailable"]);
    fixture.git([
        "config",
        "diff.fixture.textconv",
        "soxu-fixture-unavailable",
    ]);
    fixture.git(["config", "core.fsmonitor", "soxu-fixture-unavailable"]);
    fixture.write(".gitattributes", b"file diff=fixture\n");
    let before = snapshot(&fixture.temp);
    let output = fixture
        .diff(
            DiffBaseline::Uncommitted,
            &[b"file"],
            DiffOutput::Review(b"file"),
        )
        .await
        .expect("禁用外部调用");
    assert!(text(output).ends_with(b"-old\n+new\n"));
    assert_eq!(snapshot(&fixture.temp), before);
    for (key, value, expected) in [
        ("include.path", "../absent-fixture-config", "配置 include"),
        (
            "includeIf.onbranch:main.path",
            "../absent-fixture-config",
            "配置 include",
        ),
        (
            "remote.fixture.promisor",
            "true",
            "外部元数据来源或部分克隆配置",
        ),
        (
            "core.sparseCheckout",
            "true",
            "sparse checkout、sparse index 或 split index",
        ),
    ] {
        fixture.git(["config", key, value]);
        let before = snapshot(&fixture.temp);
        assert!(
            matches!(fixture.diff(DiffBaseline::Uncommitted, &[b"file"], DiffOutput::Metadata).await, Err(DiffError::Unsupported(reason)) if reason == expected)
        );
        assert_eq!(snapshot(&fixture.temp), before);
        fixture.git(["config", "--unset", key]);
    }
    fixture.git(["config", "extensions.worktreeConfig", "true"]);
    for filter in ["filter.fixture.clean", "filter.fixture.process"] {
        fixture.git(["config", "--worktree", filter, "soxu-fixture-unavailable"]);
        let before = snapshot(&fixture.temp);
        assert!(matches!(
            fixture
                .diff(DiffBaseline::Uncommitted, &[b"file"], DiffOutput::Metadata)
                .await,
            Err(DiffError::Git(GitError::ExternalFilter))
        ));
        assert_eq!(snapshot(&fixture.temp), before);
        fixture.git(["config", "--worktree", "--unset", filter]);
    }
}

#[test]
fn explicit_config_file_still_discovers_the_current_repository_without_isolation() {
    let fixture = Fixture::new(&[]);
    fixture.write(".git/config", b"[incomplete-section\n");
    fs::write(
        fixture.temp.join("separate.cfg"),
        b"[fixture]\nvalue = ordinary\n",
    )
    .expect("独立配置夹具");
    let before = snapshot(&fixture.temp);
    let in_repo = fixture
        .command(&fixture.root)
        .args([
            "--no-pager",
            "config",
            "--file",
            "../separate.cfg",
            "--no-includes",
            "--get",
            "fixture.value",
        ])
        .output()
        .expect("仓库发现对照");
    assert!(!in_repo.status.success() || !in_repo.stderr.is_empty());
    let isolated = fixture
        .command(&fixture.temp)
        .env("GIT_DIR", "separate.cfg/no-repository")
        .args([
            "--no-pager",
            "config",
            "--file",
            "separate.cfg",
            "--no-includes",
            "--get",
            "fixture.value",
        ])
        .output()
        .expect("独立配置读取");
    assert!(isolated.stderr.is_empty());
    assert_eq!(success(isolated), b"ordinary\n");
    assert_eq!(snapshot(&fixture.temp), before);
}

#[tokio::test]
async fn refuses_special_index_entries_and_object_formats() {
    let fixture = Fixture::new(&[]);
    fixture.write("file", b"old\n");
    fixture.commit();
    fixture.write("intent", b"intent\n");
    fixture.git(["add", "-N", "intent"]);
    assert!(matches!(
        fixture
            .diff(DiffBaseline::Unstaged, &[b"file"], DiffOutput::Metadata)
            .await,
        Err(DiffError::Unsupported("intent-to-add"))
    ));
    fixture.git(["rm", "--cached", "-q", "intent"]);
    fixture.git(["update-index", "--split-index"]);
    assert!(matches!(
        fixture
            .diff(DiffBaseline::Unstaged, &[b"file"], DiffOutput::Metadata)
            .await,
        Err(DiffError::Unsupported("split index"))
    ));
    let sha256 = Fixture::new(&["--object-format=sha256"]);
    assert!(matches!(
        sha256
            .diff(DiffBaseline::Unstaged, &[b"file"], DiffOutput::Metadata)
            .await,
        Err(DiffError::Unsupported("非 SHA-1 对象格式"))
    ));
}

#[tokio::test]
async fn refuses_conflicts_gitlinks_and_missing_blobs() {
    let fixture = Fixture::new(&[]);
    fixture.write("file", b"original\n");
    fixture.commit();
    let head = fixture.git(["rev-parse", "HEAD"]);
    let head = std::str::from_utf8(&head).expect("ASCII OID").trim();
    fixture.git(["branch", "side"]);
    fixture.write("file", b"main\n");
    fixture.commit();
    fixture.git(["switch", "-q", "side"]);
    fixture.write("file", b"side\n");
    fixture.commit();
    let merge = fixture
        .command(&fixture.root)
        .args(["merge", "--no-edit", "main"])
        .output()
        .expect("冲突夹具");
    assert_eq!(merge.status.code(), Some(1));
    let before = snapshot(&fixture.temp);
    let result = fixture
        .diff(DiffBaseline::Unstaged, &[b"file"], DiffOutput::Metadata)
        .await;
    assert!(
        matches!(result, Err(DiffError::Unsupported("冲突索引"))),
        "{result:?}"
    );
    assert_eq!(snapshot(&fixture.temp), before);

    let fixture = Fixture::new(&[]);
    fixture.write("file", b"original\n");
    fixture.commit();
    let commit = fixture.git(["rev-parse", "HEAD"]);
    let commit = std::str::from_utf8(&commit).expect("ASCII OID").trim();
    fixture.git([
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{commit},component"),
    ]);
    assert!(matches!(
        fixture
            .diff(DiffBaseline::Staged, &[b"file"], DiffOutput::Metadata)
            .await,
        Err(DiffError::Unsupported("子模块"))
    ));
    fixture.git(["commit", "-qm", "gitlink fixture"]);
    fixture.git(["tag", "gitlink"]);
    fixture.git(["update-index", "--force-remove", "component"]);
    assert!(matches!(
        fixture
            .diff(
                DiffBaseline::BaseToWorktree("refs/tags/gitlink"),
                &[b"file"],
                DiffOutput::Metadata
            )
            .await,
        Err(DiffError::Unsupported("子模块"))
    ));
    assert_ne!(head, commit);
    assert!(matches!(
        fixture
            .diff(
                DiffBaseline::BaseToWorktree(head),
                &[b"file"],
                DiffOutput::Metadata
            )
            .await,
        Err(DiffError::Git(GitError::CommandFailed { .. }))
    ));

    let fixture = Fixture::new(&[]);
    fixture.write("file", b"original\n");
    fixture.commit();
    fixture.write("loose", "未写入 Git 对象库的夹具");
    let hash = fixture.git(["hash-object", "loose"]);
    let hash = std::str::from_utf8(&hash).expect("ASCII OID").trim();
    fixture.git([
        "update-index",
        "--add",
        "--info-only",
        "--cacheinfo",
        &format!("100644,{hash},missing"),
    ]);
    let before = snapshot(&fixture.temp);
    let result = fixture
        .diff(DiffBaseline::Staged, &[b"missing"], DiffOutput::Metadata)
        .await;
    assert!(
        matches!(result, Err(DiffError::Git(GitError::CommandFailed { .. }))),
        "{result:?}"
    );
    assert_eq!(snapshot(&fixture.temp), before);
}

#[tokio::test]
async fn rejects_file_to_directory_replacements_and_nested_repositories() {
    let fixture = Fixture::new(&[]);
    fs::create_dir(fixture.root.join("nested")).expect("夹具目录");
    fixture.write("file", b"original\n");
    fixture.write("nested/file", b"original\n");
    fixture.commit();
    fs::rename(fixture.root.join("file"), fixture.temp.join("former-file")).expect("移动夹具文件");
    fs::create_dir(fixture.root.join("file")).expect("以夹具目录替代文件");
    fixture.write("file/child", b"untracked\n");
    success(
        fixture
            .command(&fixture.root.join("nested"))
            .args(["init", "-q", "--template="])
            .output()
            .expect("创建内嵌夹具仓库"),
    );
    let before = snapshot(&fixture.temp);
    for (path, expected) in [
        (b"file".as_slice(), "工作树路径类型变化"),
        (b"nested/file", "内嵌 Git 仓库"),
    ] {
        let result = fixture
            .diff(DiffBaseline::Uncommitted, &[path], DiffOutput::Metadata)
            .await;
        assert!(
            matches!(result, Err(DiffError::Unsupported(reason)) if reason == expected),
            "{result:?}"
        );
    }
    assert_eq!(snapshot(&fixture.temp), before);
}

#[cfg(windows)]
#[tokio::test]
async fn rejects_windows_ambiguous_names_before_git() {
    let fixture = Fixture::new(&[]);
    for path in [
        b"NUL.txt".as_slice(),
        b"COM1.txt",
        b"file. ",
        b"a\nb",
        b"invalid\xff",
        b"a*",
        b"\\\\example\\share",
        b"file:stream",
    ] {
        assert!(matches!(
            fixture
                .diff(DiffBaseline::Unstaged, &[path], DiffOutput::Metadata)
                .await,
            Err(DiffError::InvalidPath)
        ));
    }
}

#[tokio::test]
async fn fifty_thousand_lines_return_metadata_but_oversized_review_fails_closed() {
    let fixture = Fixture::new(&[]);
    let old = format!("{}\n", "a".repeat(192)).repeat(50_000);
    let new = format!("{}\n", "b".repeat(192)).repeat(50_000);
    fixture.write("large", old);
    fixture.commit();
    fixture.write("large", new);
    let before = snapshot(&fixture.temp);
    let result = fixture
        .diff(DiffBaseline::Uncommitted, &[b"large"], DiffOutput::Metadata)
        .await
        .expect("大文件元数据");
    assert_eq!(result.changes.len(), 1);
    assert!(matches!(
        fixture
            .diff(
                DiffBaseline::Uncommitted,
                &[b"large"],
                DiffOutput::Review(b"large")
            )
            .await,
        Err(DiffError::Git(GitError::OutputLimit))
    ));
    assert_eq!(snapshot(&fixture.temp), before);
}

#[cfg(unix)]
#[tokio::test]
async fn unix_preserves_raw_linebreak_name_and_refuses_a_replaced_symlink() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new(&[]);
    let path = b"line\nbyte\xff";
    let native = fixture.root.join(OsStr::from_bytes(path));
    fs::write(&native, b"old\n").expect("原始路径夹具");
    fixture.commit();
    fs::write(&native, b"new\n").expect("修改原始路径夹具");
    let result = fixture
        .diff(DiffBaseline::Unstaged, &[path], DiffOutput::Review(path))
        .await
        .expect("Unix 原始路径");
    assert_eq!(result.changes[0].path, path);
    fs::rename(&native, fixture.temp.join("separate")).expect("移至临时根");
    symlink(fixture.temp.join("separate"), &native).expect("建立夹具链接");
    assert!(matches!(
        fixture
            .diff(DiffBaseline::Unstaged, &[path], DiffOutput::Metadata)
            .await,
        Err(DiffError::Unsupported("符号链接或 reparse point"))
    ));
}
