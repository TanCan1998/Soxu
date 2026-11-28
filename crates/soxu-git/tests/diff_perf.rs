//! 手动 50,000 行 Diff 压测：默认 `cargo test` 忽略。
//!
//! 运行：`cargo test -p soxu-git --test diff_perf -- --ignored --nocapture`
//! 输出记录墙钟耗时、patch 字节与构造输入字节；进程峰值内存由外部进程监测器
//!（PowerShell `Get-Process.PeakWorkingSet64`）采样并写入 PR。这里不放不稳定耗时/内存阈值进 CI。
//!
//! 固定夹具结构：单一已跟踪文本文件，初始 50,000 行；第二版本逐行替换全部行；
//! 只读 Diff 仅授权该文件并返回 patch。断言既有 `DiffChange` 元数据与完整 patch
//! 输出语义，没有重新实现或放宽 `soxu-git` 的时间/输出上限。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use soxu_git::{DiffBaseline, DiffKind, DiffOutput, DiffRequest, DiffReview, GitProbe};

const LINE_COUNT: usize = 50_000;
const LINE_BYTES: usize = 80;

struct Fixture {
    root: PathBuf,
    temp: PathBuf,
    git: GitProbe,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let git = GitProbe::discover().expect("本机 Git");
        // 与 `tests/diff.rs` 同口径：自建专属临时根，析构时自行回收。
        let temp = std::env::temp_dir().join(format!(
            "soxu-diff-perf-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&temp).expect("独立临时根");
        let root = temp.join("repo");
        fs::create_dir(&root).expect("夹具仓库目录");
        let fixture = Self { root, temp, git };
        fixture.git(&["init", "--quiet", "--template=", "--initial-branch=main"]);
        fixture
    }

    fn command(&self, root: &Path) -> Command {
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
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .args([
                "-c",
                "user.name=50k Diff Fixture",
                "-c",
                "user.email=diff-50k@example.invalid",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ]);
        command
    }

    fn git(&self, args: &[&str]) {
        let output = self
            .command(&self.root)
            .args(args)
            .output()
            .expect("启动夹具 Git");
        assert!(
            output.status.success(),
            "夹具 Git 失败：args={args:?}, exit={:?}, stderr={} bytes",
            output.status.code(),
            output.stderr.len()
        );
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "--all"]);
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let timestamp = format!(
            "{} +0000",
            1_800_000_000 + SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let output = self
            .command(&self.root)
            .env("GIT_AUTHOR_DATE", &timestamp)
            .env("GIT_COMMITTER_DATE", &timestamp)
            .args(["commit", "--quiet", "-m", message])
            .output()
            .expect("提交夹具版本");
        assert!(
            output.status.success(),
            "夹具提交失败：exit={:?}, stderr={} bytes",
            output.status.code(),
            output.stderr.len()
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // 只回收本夹具自己创建的唯一临时根，不碰系统临时目录其他内容。
        let _ = fs::remove_dir_all(&self.temp);
    }
}

fn line(version: u8, index: usize) -> String {
    let prefix = format!("v{version} row={index:05} payload=");
    let mut value = prefix;
    while value.len() + 1 < LINE_BYTES {
        value.push(char::from(
            b'a' + u8::try_from(index % 26).expect("字母索引"),
        ));
    }
    value.push('\n');
    value
}

fn fixture_bytes(version: u8) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((LINE_COUNT + 1) * LINE_BYTES);
    for index in 0..LINE_COUNT {
        bytes.extend_from_slice(line(version, index).as_bytes());
    }
    bytes
}

#[test]
#[ignore = "手动 50,000 行性能基线；CI 不承载不稳定时间/内存阈值"]
fn fifty_thousand_replacements_preserve_bounded_diff_output_contract() {
    let fixture = Fixture::new();
    let path = fixture.root.join("large-review.txt");
    let initial = fixture_bytes(1);
    let initial_bytes = initial.len();
    fs::write(&path, initial).expect("写入 50,000 行基准版本");
    fixture.commit("baseline 50k lines");

    let changed = fixture_bytes(2);
    fs::write(&path, changed).expect("替换 50,000 行");
    let git_path = b"large-review.txt";
    let started = Instant::now();
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(fixture.git.readonly_diff(
            &fixture.root,
            DiffRequest {
                baseline: DiffBaseline::Unstaged,
                paths: &[git_path],
                output: DiffOutput::Review(git_path),
            },
        ))
        .expect("50,000 行 diff 必须在现有请求预算内完成");
    let elapsed = started.elapsed();

    assert_eq!(result.changes.len(), 1, "授权文件只返回一项变化");
    assert_eq!(result.changes[0].kind, DiffKind::Modified);
    assert_eq!(result.changes[0].path, git_path);
    let Some(DiffReview::Text(patch)) = result.review else {
        panic!("纯文本替换必须返回完整文本 patch");
    };
    assert!(!patch.is_empty(), "patch 不得被静默截断为空");
    assert!(!patch.contains(&0), "文本 patch 不得被识别成二进制");
    let patch_text = std::str::from_utf8(&patch).expect("UTF-8 patch");
    let additions = patch_text
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++ "))
        .count();
    let deletions = patch_text
        .lines()
        .filter(|line| line.starts_with('-') && !line.starts_with("--- "))
        .count();
    assert_eq!(additions, LINE_COUNT, "必须保留全部新增行");
    assert_eq!(deletions, LINE_COUNT, "必须保留全部删除行");
    assert!(
        patch.len() < 16 * 1024 * 1024,
        "本夹具須落在既有單命令输出上限内；实际 {} bytes",
        patch.len()
    );

    eprintln!(
        "SOXU_DIFF_PERF lines={LINE_COUNT} input_bytes={initial_bytes} patch_bytes={} elapsed_ms={} output_limit_bytes=16777216",
        patch.len(),
        elapsed.as_millis(),
    );
}
