#![cfg(any(target_os = "linux", windows))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use soxu_git::{
    CheckpointError, CheckpointLifecycle, CheckpointOptions, CheckpointPolicy, CheckpointRecord,
    CheckpointScope, CheckpointStage, GitProbe, OmissionReason,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    state: PathBuf,
}

impl Fixture {
    fn new(format: &str) -> Self {
        let base = fs::canonicalize(std::env::temp_dir()).expect("临时目录");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("时钟")
            .as_nanos();
        let dir = base.join(format!(
            "soxu-checkpoint-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).expect("夹具目录");
        let root = dir.join("worktree");
        let state = dir.join("state");
        fs::create_dir(&root).expect("工作树目录");
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&state)
                .expect("独占应用状态目录");
        }
        #[cfg(windows)]
        fs::create_dir(&state).expect("独占应用状态目录");
        git(
            &root,
            &["init", "--initial-branch=main", "--object-format", format],
        );
        git(&root, &["config", "user.name", "Fixture"]);
        git(&root, &["config", "user.email", "fixture@soxu.invalid"]);
        git(&root, &["config", "core.autocrlf", "false"]);
        Self { dir, root, state }
    }

    fn write(&self, path: &str, content: impl AsRef<[u8]>) {
        fs::write(self.root.join(path), content).expect("写入夹具");
    }

    fn commit(&self) {
        git(&self.root, &["add", "--", "."]);
        git(&self.root, &["commit", "-m", "fixture"]);
    }

    fn options(&self) -> CheckpointOptions {
        CheckpointOptions {
            state_directory: self.state.clone(),
            scope: CheckpointScope::new(),
            turn_index: 7,
            policy: CheckpointPolicy::default(),
        }
    }

    fn operation(&self) -> PathBuf {
        fs::read_dir(&self.state)
            .expect("列举状态目录")
            .map(|entry| entry.expect("操作目录").path())
            .find(|path| path.join("intent").is_file())
            .expect("持久 Intent")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn command(root: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            cmd.env_remove(key);
        }
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args(args)
        .output()
        .expect("运行夹具 Git")
}

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = command(root, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn probe() -> GitProbe {
    GitProbe::discover().expect("可信 Git")
}
fn show(root: &Path, record: &CheckpointRecord, path: &str) -> Vec<u8> {
    git(root, &["show", &format!("{}:{path}", record.commit_oid)])
}
fn omitted(record: &CheckpointRecord, path: &str, reason: OmissionReason) -> bool {
    record
        .coverage
        .omissions
        .iter()
        .any(|entry| entry.path == path.as_bytes() && entry.reason == reason)
}

#[tokio::test]
async fn staged_bytes_and_reflogs_are_unchanged() {
    let f = Fixture::new("sha1");
    f.write("note.txt", b"base\n");
    f.write("delete.txt", b"delete me\n");
    f.commit();
    f.write("note.txt", b"stage\n");
    git(&f.root, &["add", "--", "note.txt"]);
    f.write("note.txt", b"live\n");
    fs::remove_file(f.root.join("delete.txt")).expect("删除夹具路径");
    let observe = || {
        [
            ".git/HEAD",
            ".git/index",
            ".git/logs/HEAD",
            ".git/logs/refs/heads/main",
            "note.txt",
        ]
        .map(|p| fs::read(f.root.join(p)).expect("观测夹具字节"))
    };
    let before = observe();
    let result = probe()
        .capture_checkpoint(&f.root, &f.options())
        .await
        .expect("捕获工作树");
    assert_eq!(observe(), before);
    assert_eq!(show(&f.root, &result, "note.txt"), b"live\n");
    assert!(
        !command(
            &f.root,
            &[
                "cat-file",
                "-e",
                &format!("{}:delete.txt", result.commit_oid)
            ]
        )
        .status
        .success()
    );
    assert_eq!(git(&f.root, &["show", ":note.txt"]), b"stage\n");
    assert_eq!(result.summary().changed_files, 2);
    assert!(!f.root.join(".git/logs/refs/soxu").exists());
    assert_eq!(
        probe()
            .inspect_checkpoint(&f.root, f.operation())
            .await
            .expect("重开记录"),
        result
    );
}

#[tokio::test]
async fn coverage_contains_ignore_budget_filter_sensitive_and_binary_outcomes() {
    let f = Fixture::new("sha1");
    f.write(".gitignore", "ignore.me\n");
    fs::write(f.root.join(".git/info/exclude"), "excluded.txt\n").expect("仓内 exclude");
    f.write(".gitattributes", "filtered.txt filter=poison\n");
    git(
        &f.root,
        &[
            "config",
            "filter.poison.clean",
            "echo executed > filter-ran.flag",
        ],
    );
    git(
        &f.root,
        &[
            "config",
            "filter.poison.process",
            "echo executed > filter-ran.flag",
        ],
    );
    f.write("- 空 格.txt", "通过\n");
    f.write("ignore.me", "ignored\n");
    f.write("excluded.txt", "excluded\n");
    f.write("large.txt", vec![b'x'; 150]);
    f.write("binary", [0, 4, 5]);
    f.write(".env", "fixture content\n");
    f.write("suspect", "password=fixture\n");
    f.write("filtered.txt", "filter required\n");
    let mut options = f.options();
    options.policy.max_text_bytes = 128;
    let result = probe()
        .capture_checkpoint(&f.root, &options)
        .await
        .expect("捕获并记录遗漏");
    assert_eq!(show(&f.root, &result, "- 空 格.txt"), "通过\n".as_bytes());
    for (path, reason) in [
        ("ignore.me", OmissionReason::Ignored),
        ("excluded.txt", OmissionReason::Ignored),
        ("large.txt", OmissionReason::TextBudget),
        ("binary", OmissionReason::BinaryNotAuthorized),
        (".env", OmissionReason::SensitivePath),
        ("suspect", OmissionReason::SensitiveContent),
        ("filtered.txt", OmissionReason::UnsupportedFilter),
    ] {
        assert!(
            omitted(&result, path, reason),
            "遗漏 {path}: {:?}",
            result.coverage
        );
    }
    assert!(!f.root.join("filter-ran.flag").exists());
    assert!(!result.coverage.complete);
    options.policy.allow_binary = true;
    let result = probe()
        .capture_checkpoint(&f.root, &options)
        .await
        .expect("授权捕获二进制");
    assert_eq!(show(&f.root, &result, "binary"), [0, 4, 5]);
    options.policy.max_paths = 1;
    let result = probe()
        .capture_checkpoint(&f.root, &options)
        .await
        .expect("部分覆盖");
    assert!(
        result
            .coverage
            .omissions
            .iter()
            .any(|entry| entry.reason == OmissionReason::PathBudget)
    );
}

#[tokio::test]
async fn sha256_unborn_and_linked_detached_worktree() {
    let f = Fixture::new("sha256");
    f.write("note.txt", b"unborn\n");
    let record = probe()
        .capture_checkpoint(&f.root, &f.options())
        .await
        .expect("未出生 SHA-256");
    assert_eq!(record.base_tree_oid.len(), 64);
    assert!(record.parent_oid.is_none());
    assert_eq!(git(&f.root, &["ls-tree", &record.base_tree_oid]), b"");
    assert!(
        !command(&f.root, &["rev-parse", "--verify", "HEAD"])
            .status
            .success()
    );
    f.commit();
    let linked = f.dir.join("linked");
    git(
        &f.root,
        &[
            "worktree",
            "add",
            "--detach",
            {
                let path = linked.to_str().expect("路径");
                path.strip_prefix(r"\\?\").unwrap_or(path)
            },
            "HEAD",
        ],
    );
    fs::write(linked.join("note.txt"), b"linked\n").expect("修改 linked");
    let record = probe()
        .capture_checkpoint(&linked, &f.options())
        .await
        .expect("linked worktree");
    assert_ne!(record.before.git_dir, record.before.common_dir);
    assert_eq!(record.before.head.first(), Some(&0));
    assert!(record.parent_oid.is_some());
    assert_eq!(show(&linked, &record, "note.txt"), b"linked\n");
    assert_eq!(
        fs::read(f.root.join("note.txt")).expect("主工作树"),
        b"unborn\n"
    );
}

#[tokio::test]
async fn bare_unmerged_and_state_inside_worktree_are_rejected() {
    let f = Fixture::new("sha1");
    f.write("file", "base\n");
    f.commit();
    let bare = f.dir.join("bare");
    fs::create_dir(&bare).expect("bare 目录");
    git(&bare, &["init", "--bare"]);
    assert!(matches!(
        probe().capture_checkpoint(&bare, &f.options()).await,
        Err(CheckpointError::Invalid(_))
    ));
    let mut options = f.options();
    options.state_directory = f.root.clone();
    assert!(probe().capture_checkpoint(&f.root, &options).await.is_err());
    let files = fs::read_dir(&f.root).expect("工作树清单").count();
    assert_eq!(files, 2);
    git(&f.root, &["switch", "-c", "other"]);
    f.write("file", "other\n");
    f.commit();
    git(&f.root, &["switch", "main"]);
    f.write("file", "main\n");
    f.commit();
    assert!(!command(&f.root, &["merge", "other"]).status.success());
    fs::remove_file(f.root.join(".git/MERGE_HEAD")).expect("单独测试 Unmerged");
    assert!(matches!(
        probe().capture_checkpoint(&f.root, &f.options()).await,
        Err(CheckpointError::Invalid("Unmerged index"))
    ));
}

#[tokio::test]
async fn content_head_and_index_drift_block_publication() {
    for target in ["content", "head", "index"] {
        let f = Fixture::new("sha1");
        f.write("file", "base\n");
        f.commit();
        f.write("file", "current\n");
        let options = f.options();
        let result = probe()
            .capture_checkpoint_observed(&f.root, &options, |stage| {
                if stage != CheckpointStage::Captured {
                    return;
                }
                match target {
                    "content" => f.write("file", "drift\n"),
                    "head" => {
                        git(&f.root, &["symbolic-ref", "HEAD", "refs/heads/new"]);
                    }
                    "index" => {
                        git(&f.root, &["add", "--", "file"]);
                    }
                    _ => unreachable!(),
                }
            })
            .await;
        assert!(
            matches!(result, Err(CheckpointError::Drift)),
            "{target}: {result:?}"
        );
        assert_eq!(
            git(&f.root, &["for-each-ref", "refs/soxu/checkpoints"]),
            b""
        );
    }
}

#[tokio::test]
async fn cas_conflict_preserves_different_ref() {
    let f = Fixture::new("sha1");
    f.write("file", "base\n");
    f.commit();
    f.write("file", "modified\n");
    let options = f.options();
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    let mut conflicting = String::new();
    let result = probe()
        .capture_checkpoint_observed(&f.root, &options, |stage| {
            if stage == CheckpointStage::IntentDurable {
                let directory = f.operation();
                conflicting = format!(
                    "refs/soxu/checkpoints/{}/{}/{}",
                    options.scope.workspace_id(),
                    options.scope.session_id(),
                    directory.file_name().expect("ID").to_string_lossy()
                );
                git(&f.root, &["update-ref", &conflicting, "HEAD"]);
            }
        })
        .await;
    assert!(
        matches!(result, Err(CheckpointError::CasConflict)),
        "{result:?}"
    );
    assert_eq!(git(&f.root, &["rev-parse", &conflicting]), head);
    assert!(matches!(
        probe().inspect_checkpoint(&f.root, f.operation()).await,
        Err(CheckpointError::CasConflict)
    ));
}

#[tokio::test]
async fn durable_intent_recovers_when_record_write_is_interrupted() {
    let f = Fixture::new("sha1");
    f.write("untracked", "first\n");
    let options = f.options();
    let result = probe()
        .capture_checkpoint_observed(&f.root, &options, |stage| {
            if stage == CheckpointStage::IntentDurable {
                fs::write(
                    f.operation().join("record.pending"),
                    b"interrupted publication",
                )
                .expect("模拟进程中断");
            }
        })
        .await;
    assert!(matches!(result, Err(CheckpointError::Io(_))), "{result:?}");
    let reopened = probe()
        .inspect_checkpoint(&f.root, f.operation())
        .await
        .expect("重启只读核验");
    assert_eq!(reopened.lifecycle, CheckpointLifecycle::Published);
    assert_eq!(show(&f.root, &reopened, "untracked"), b"first\n");
    assert!(!f.operation().join("record").exists());
}

#[cfg(windows)]
#[tokio::test]
async fn windows_holds_directories_without_delete_sharing() {
    let f = Fixture::new("sha1");
    f.write("note.txt", "checkpoint\n");
    let outside = f.dir.join("outside");
    fs::create_dir(&outside).expect("外部夹具目录");
    fs::write(outside.join("hidden"), "outside\n").expect("外部夹具字节");
    let reparse = match std::os::windows::fs::symlink_dir(&outside, f.root.join("redirect")) {
        Ok(()) => true,
        Err(error) if error.raw_os_error() == Some(1314) => false,
        Err(error) => panic!("创建 Windows reparse 夹具失败：{error}"),
    };
    let captured = probe()
        .capture_checkpoint_observed(&f.root, &f.options(), |stage| {
            if stage == CheckpointStage::Captured {
                assert!(fs::rename(&f.root, f.dir.join("move-attempt")).is_err());
            }
        })
        .await
        .expect("Windows 固定目录捕获");
    assert!(
        captured
            .before
            .platform
            .contains("stable-file-id-and-nlink-unavailable")
    );
    if reparse {
        assert!(omitted(&captured, "redirect", OmissionReason::SpecialFile));
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn linux_rejects_links_fifos_nested_repositories_and_preserves_raw_path_bytes() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    let f = Fixture::new("sha1");
    fs::write(f.dir.join("outside"), "outside\n").expect("创建越界夹具");
    symlink(f.dir.join("outside"), f.root.join("symlink")).expect("设置符号链接");
    fs::hard_link(f.dir.join("outside"), f.root.join("hard-link")).expect("设置硬链接");
    let fifo = Command::new("mkfifo")
        .arg(f.root.join("fifo"))
        .status()
        .expect("创建 FIFO");
    assert!(fifo.success());
    fs::create_dir(f.root.join("nested")).expect("嵌套目录");
    git(&f.root.join("nested"), &["init"]);
    f.write("nested/private", "nested\n");
    let raw = b"raw-\xff";
    fs::write(f.root.join(OsStr::from_bytes(raw)), "text\n").expect("非 UTF-8 路径");
    let captured = probe()
        .capture_checkpoint(&f.root, &f.options())
        .await
        .expect("只捕获允许路径");
    for name in ["symlink", "hard-link", "fifo"] {
        assert!(omitted(&captured, name, OmissionReason::SpecialFile));
    }
    assert!(omitted(
        &captured,
        "nested",
        OmissionReason::NestedRepository
    ));
    assert!(
        captured
            .coverage
            .included
            .iter()
            .any(|item| item.path == raw)
    );
}
