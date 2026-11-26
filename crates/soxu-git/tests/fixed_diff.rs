#![cfg(any(target_os = "linux", windows))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use soxu_git::{
    CheckpointOptions, CheckpointPolicy, CheckpointRecord, CheckpointScope, DiffOutput, DiffReview,
    FixedDiffChange, FixedDiffEndpoint, FixedDiffError, FixedDiffIncomplete, FixedDiffKind,
    FixedDiffRequest, FixedDiffResult, GitProbe,
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
            "soxu-fixed-diff-{}-{stamp}-{}",
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
        let target = self.root.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("夹具目录链");
        }
        fs::write(target, content).expect("写入夹具");
    }

    fn commit(&self) {
        git(&self.root, &["add", "--", "."]);
        git(&self.root, &["commit", "-m", "fixture"]);
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

fn git_oid(root: &Path, reference: &str) -> String {
    String::from_utf8(git(root, &["rev-parse", reference]))
        .expect("OID 文本")
        .trim()
        .to_owned()
}

fn probe() -> GitProbe {
    GitProbe::discover().expect("可信 Git")
}

async fn capture(f: &Fixture, scope: &CheckpointScope, turn: u32) -> CheckpointRecord {
    capture_with(f, scope, turn, CheckpointPolicy::default()).await
}

async fn capture_with(
    f: &Fixture,
    scope: &CheckpointScope,
    turn: u32,
    policy: CheckpointPolicy,
) -> CheckpointRecord {
    probe()
        .capture_checkpoint(
            &f.root,
            &CheckpointOptions {
                state_directory: f.state.clone(),
                scope: scope.clone(),
                turn_index: turn,
                policy,
            },
        )
        .await
        .expect("捕获检查点")
}

fn checkpoint<'a>(
    f: &'a Fixture,
    scope: &'a CheckpointScope,
    record: &'a CheckpointRecord,
) -> FixedDiffEndpoint<'a> {
    FixedDiffEndpoint::AuthorizedCheckpoint {
        state_directory: f.state.as_path(),
        scope,
        checkpoint_id: record.checkpoint_id.as_str(),
    }
}

fn change<'a>(result: &'a FixedDiffResult, path: &str) -> &'a FixedDiffChange {
    result
        .changes
        .iter()
        .find(|change| change.path == path.as_bytes())
        .unwrap_or_else(|| panic!("缺少变更 {path}: {:?}", result.changes))
}

fn is_rejected(error: &FixedDiffError) -> bool {
    matches!(
        error,
        FixedDiffError::UnauthorizedCheckpoint | FixedDiffError::Checkpoint(_)
    )
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>, SystemTime)> {
    fn walk(root: &Path, base: &Path, out: &mut Vec<(PathBuf, Vec<u8>, SystemTime)>) {
        let mut entries = fs::read_dir(root)
            .expect("目录")
            .map(|entry| entry.expect("条目"))
            .collect::<Vec<_>>();
        entries.sort_by_key(fs::DirEntry::path);
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).expect("元数据");
            if metadata.is_dir() {
                walk(&path, base, out);
            } else {
                out.push((
                    path.strip_prefix(base).expect("相对路径").to_owned(),
                    fs::read(&path).expect("文件字节"),
                    metadata.modified().expect("修改时间"),
                ));
            }
        }
    }
    let mut result = Vec::new();
    walk(root, root, &mut result);
    result
}

#[tokio::test]
async fn checkpoint_pair_and_base_commit_endpoints_are_independent() {
    let f = Fixture::new("sha1");
    f.write("keep.txt", b"keep");
    f.write("change.txt", b"version-one");
    f.write("removed.txt", b"removed");
    f.commit();
    let base_commit = git_oid(&f.root, "HEAD");
    let scope = CheckpointScope::new();
    f.write("change.txt", b"version-two");
    let first = capture(&f, &scope, 1).await;
    f.write("change.txt", b"version-three");
    fs::remove_file(f.root.join("removed.txt")).expect("删除夹具路径");
    let second = capture(&f, &scope, 2).await;

    let result = probe()
        .readonly_fixed_diff(
            &f.root,
            FixedDiffRequest {
                base: checkpoint(&f, &scope, &first),
                head: checkpoint(&f, &scope, &second),
                paths: &[],
                output: DiffOutput::Metadata,
            },
        )
        .await
        .expect("比较两个检查点");
    assert!(result.is_complete(), "{:?}", result.incomplete_reason);
    assert_eq!(
        result.base.checkpoint_id.as_deref(),
        Some(first.checkpoint_id.as_str())
    );
    assert_eq!(result.base.session_id.as_deref(), Some(scope.session_id()));
    assert_eq!(
        result.base.workspace_id.as_deref(),
        Some(scope.workspace_id())
    );
    assert_eq!(
        result.base.commit_oid.as_deref(),
        Some(first.commit_oid.as_str())
    );
    assert_eq!(
        result.base.tree_oid.as_deref(),
        Some(first.worktree_tree_oid.as_str())
    );
    assert!(!result.base.mutable);
    assert!(!result.head.mutable);
    assert_eq!(
        result
            .base_coverage
            .checkpoint
            .as_ref()
            .map(|manifest| manifest.complete),
        Some(true)
    );
    assert_eq!(
        result.selected_paths,
        vec![
            b"change.txt".to_vec(),
            b"keep.txt".to_vec(),
            b"removed.txt".to_vec()
        ]
    );
    assert_eq!(result.changes.len(), 2);
    let modified = change(&result, "change.txt");
    assert_eq!(modified.kind, FixedDiffKind::Modified);
    assert_eq!(modified.additions, Some(1));
    assert_eq!(modified.deletions, Some(1));
    assert!(modified.old_oid.is_some() && modified.new_oid.is_some());
    assert_eq!(change(&result, "removed.txt").kind, FixedDiffKind::Deleted);

    let result = probe()
        .readonly_fixed_diff(
            &f.root,
            FixedDiffRequest {
                base: FixedDiffEndpoint::AuthorizedBaseCommit(&base_commit),
                head: checkpoint(&f, &scope, &second),
                paths: &[],
                output: DiffOutput::Review(b"change.txt"),
            },
        )
        .await
        .expect("比较本地提交与检查点");
    assert!(result.is_complete(), "{:?}", result.incomplete_reason);
    assert_eq!(result.base.checkpoint_id, None);
    assert_eq!(
        result.base.commit_oid.as_deref(),
        Some(base_commit.as_str())
    );
    assert!(result.base.workspace_id.is_none());
    assert_eq!(change(&result, "removed.txt").kind, FixedDiffKind::Deleted);
    let Some(DiffReview::Text(patch)) = &result.review else {
        panic!("期望文本审阅: {:?}", result.review);
    };
    let patch = String::from_utf8_lossy(patch);
    assert!(patch.contains("version-three"), "{patch}");
    assert!(!patch.contains("keep.txt"), "{patch}");
}

#[tokio::test]
async fn checkpoint_to_worktree_is_a_mutable_observation_with_omissions() {
    let f = Fixture::new("sha1");
    f.write(".gitignore", b"cache.me");
    f.write("change.txt", b"one");
    f.commit();
    let scope = CheckpointScope::new();
    let record = capture(&f, &scope, 1).await;
    f.write("change.txt", b"two");
    f.write("scratch.txt", b"untracked");
    f.write("cache.me", b"ignored");

    let result = probe()
        .readonly_checkpoint_diff(
            &f.root,
            checkpoint(&f, &scope, &record),
            &[],
            DiffOutput::Metadata,
        )
        .await
        .expect("比较检查点与工作树");
    assert!(result.head.mutable);
    assert_eq!(result.head.commit_oid, None);
    assert_eq!(result.head.tree_oid, None);
    assert_eq!(result.head.checkpoint_id, None);
    assert_eq!(
        result.head.worktree.root,
        fs::canonicalize(&f.root).expect("规范化工作树")
    );
    assert!(
        result
            .incomplete_reason
            .contains(&FixedDiffIncomplete::UntrackedWorktreePaths)
    );
    assert!(
        result
            .head_coverage
            .untracked_paths
            .contains(&b"scratch.txt".to_vec())
    );
    assert!(
        result
            .head_coverage
            .untracked_paths
            .contains(&b"cache.me".to_vec())
    );
    assert!(
        !result
            .head_coverage
            .untracked_paths
            .contains(&b"change.txt".to_vec())
    );
    assert_eq!(change(&result, "change.txt").kind, FixedDiffKind::Modified);
    assert_eq!(change(&result, "change.txt").additions, Some(1));

    let error = probe()
        .readonly_checkpoint_diff(
            &f.root,
            FixedDiffEndpoint::AuthorizedBaseCommit(&record.commit_oid),
            &[],
            DiffOutput::Metadata,
        )
        .await
        .expect_err("工作树比较必须要求已授权检查点");
    assert!(matches!(error, FixedDiffError::UnauthorizedCheckpoint));
}

#[tokio::test]
async fn unauthorized_checkpoints_and_commit_strings_are_rejected() {
    let f = Fixture::new("sha1");
    f.write("note.txt", b"base");
    f.commit();
    let scope = CheckpointScope::new();
    f.write("note.txt", b"live");
    let record = capture(&f, &scope, 1).await;
    fn request<'a>(base: FixedDiffEndpoint<'a>, head: &'a str) -> FixedDiffRequest<'a> {
        FixedDiffRequest {
            base,
            head: FixedDiffEndpoint::AuthorizedBaseCommit(head),
            paths: &[],
            output: DiffOutput::Metadata,
        }
    }

    let other = scope.new_session();
    let error = probe()
        .readonly_checkpoint_diff(
            &f.root,
            FixedDiffEndpoint::AuthorizedCheckpoint {
                state_directory: f.state.as_path(),
                scope: &other,
                checkpoint_id: record.checkpoint_id.as_str(),
            },
            &[],
            DiffOutput::Metadata,
        )
        .await
        .expect_err("跨会话检查点");
    assert!(is_rejected(&error), "{error:?}");

    let stranger = Fixture::new("sha1");
    stranger.write("note.txt", b"other");
    stranger.commit();
    let error = probe()
        .readonly_checkpoint_diff(
            &stranger.root,
            checkpoint(&f, &scope, &record),
            &[],
            DiffOutput::Metadata,
        )
        .await
        .expect_err("跨工作树检查点");
    assert!(is_rejected(&error), "{error:?}");

    for skip in ["missing-checkpoint", "../escape", "with space"] {
        let error = probe()
            .readonly_checkpoint_diff(
                &f.root,
                FixedDiffEndpoint::AuthorizedCheckpoint {
                    state_directory: f.state.as_path(),
                    scope: &scope,
                    checkpoint_id: skip,
                },
                &[],
                DiffOutput::Metadata,
            )
            .await
            .expect_err("非法检查点标识");
        assert!(is_rejected(&error), "{skip}: {error:?}");
    }

    let shortened = record.commit_oid[..7].to_owned();
    for reference in [
        shortened.as_str(),
        "HEAD",
        "refs/heads/main",
        "refs/soxu/checkpoints",
    ] {
        let error = probe()
            .readonly_fixed_diff(
                &f.root,
                request(
                    FixedDiffEndpoint::AuthorizedBaseCommit(reference),
                    &record.commit_oid,
                ),
            )
            .await
            .expect_err("只接受完整 OID");
        assert!(
            matches!(error, FixedDiffError::InvalidCommit),
            "{reference}: {error:?}"
        );
    }

    let error = probe()
        .readonly_fixed_diff(
            &f.root,
            FixedDiffRequest {
                base: FixedDiffEndpoint::AuthorizedBaseCommit(&record.commit_oid),
                head: checkpoint(&f, &scope, &record),
                paths: &[],
                output: DiffOutput::Metadata,
            },
        )
        .await
        .expect_err("私有 ref 提交不是公开基提交");
    assert!(
        matches!(error, FixedDiffError::UnreachableCommit),
        "{error:?}"
    );

    git(&f.root, &["commit", "--allow-empty", "-m", "drift"]);
    let drifted = git_oid(&f.root, "HEAD");
    let reference = record.reference();
    git(&f.root, &["update-ref", &reference, &drifted]);
    let error = probe()
        .readonly_checkpoint_diff(
            &f.root,
            checkpoint(&f, &scope, &record),
            &[],
            DiffOutput::Metadata,
        )
        .await
        .expect_err("私有 ref OID 漂移");
    assert!(is_rejected(&error), "{error:?}");

    git(&f.root, &["update-ref", "-d", &reference]);
    let error = probe()
        .readonly_checkpoint_diff(
            &f.root,
            checkpoint(&f, &scope, &record),
            &[],
            DiffOutput::Metadata,
        )
        .await
        .expect_err("私有 ref 被删除");
    assert!(is_rejected(&error), "{error:?}");
}

#[tokio::test]
async fn checkpoint_omissions_reach_the_result_envelope() {
    let f = Fixture::new("sha1");
    f.write(".gitignore", b"ignore.me");
    f.write("large.txt", vec![b'x'; 200]);
    f.write("note.txt", b"base");
    f.commit();
    let scope = CheckpointScope::new();
    let policy = CheckpointPolicy {
        max_text_bytes: 128,
        ..CheckpointPolicy::default()
    };
    let limited = capture_with(&f, &scope, 1, policy).await;
    assert!(!limited.coverage.complete);
    let full = capture(&f, &scope, 2).await;

    let result = probe()
        .readonly_fixed_diff(
            &f.root,
            FixedDiffRequest {
                base: checkpoint(&f, &scope, &limited),
                head: checkpoint(&f, &scope, &full),
                paths: &[],
                output: DiffOutput::Metadata,
            },
        )
        .await
        .expect("比较含遗漏的检查点");
    assert!(!result.is_complete());
    assert!(
        result
            .incomplete_reason
            .contains(&FixedDiffIncomplete::BaseOmissions)
    );
    let manifest = result
        .base_coverage
        .checkpoint
        .as_ref()
        .expect("基准端覆盖清单");
    assert!(
        manifest
            .omissions
            .iter()
            .any(|entry| entry.path == b"large.txt")
    );
    assert_eq!(
        result
            .head_coverage
            .checkpoint
            .as_ref()
            .map(|manifest| manifest.complete),
        Some(true)
    );
    assert!(
        !result
            .incomplete_reason
            .contains(&FixedDiffIncomplete::HeadOmissions)
    );
}

#[tokio::test]
async fn path_selection_and_budgets_are_reported_not_hidden() {
    let f = Fixture::new("sha1");
    for index in 0..130 {
        f.write(&format!("file-{index:03}.txt"), b"content");
    }
    f.commit();
    let base_commit = git_oid(&f.root, "HEAD");
    let scope = CheckpointScope::new();
    f.write("file-000.txt", b"changed");
    let record = capture(&f, &scope, 1).await;
    let compare = |paths: &'static [&'static [u8]]| FixedDiffRequest {
        base: FixedDiffEndpoint::AuthorizedBaseCommit(&base_commit),
        head: checkpoint(&f, &scope, &record),
        paths,
        output: DiffOutput::Metadata,
    };

    let result = probe()
        .readonly_fixed_diff(&f.root, compare(&[b"file-000.txt" as &[u8]]))
        .await
        .expect("部分授权");
    assert!(
        result
            .incomplete_reason
            .contains(&FixedDiffIncomplete::PathSelection)
    );
    assert_eq!(result.selected_paths, vec![b"file-000.txt".to_vec()]);
    assert_eq!(result.changes.len(), 1);

    let result = probe()
        .readonly_fixed_diff(&f.root, compare(&[]))
        .await
        .expect("全路径比较");
    assert!(
        result
            .incomplete_reason
            .contains(&FixedDiffIncomplete::PathBudget)
    );
    assert_eq!(result.selected_paths.len(), 128);
    assert!(!result.is_complete());

    let error = probe()
        .readonly_fixed_diff(&f.root, compare(&[b"untracked.txt" as &[u8]]))
        .await
        .expect_err("未跟踪路径");
    assert!(matches!(error, FixedDiffError::Diff(_)), "{error:?}");

    let error = probe()
        .readonly_fixed_diff(
            &f.root,
            compare(&[b"file-000.txt" as &[u8], b"file-000.txt" as &[u8]]),
        )
        .await
        .expect_err("重复路径");
    assert!(matches!(error, FixedDiffError::Diff(_)), "{error:?}");

    let error = probe()
        .readonly_fixed_diff(&f.root, compare(&[b"../escape.txt" as &[u8]]))
        .await
        .expect_err("越界路径");
    assert!(matches!(error, FixedDiffError::Diff(_)), "{error:?}");
}

#[tokio::test]
async fn sha256_binary_rename_and_readonly_boundaries() {
    let f = Fixture::new("sha256");
    f.write("keep.txt", b"keep");
    f.write("old-name.txt", b"rename me");
    f.write("changed.txt", b"one");
    f.write("bin.dat", [0, 1, 2]);
    f.commit();
    let base_commit = git_oid(&f.root, "HEAD");
    assert_eq!(base_commit.len(), 64);
    let scope = CheckpointScope::new();
    git(&f.root, &["mv", "old-name.txt", "new-name.txt"]);
    f.write("changed.txt", b"one-two");
    f.write("bin.dat", [0, 9, 9]);
    let policy = CheckpointPolicy {
        allow_binary: true,
        ..CheckpointPolicy::default()
    };
    let record = capture_with(&f, &scope, 1, policy).await;
    let before = snapshot(&f.root);

    let result = probe()
        .readonly_fixed_diff(
            &f.root,
            FixedDiffRequest {
                base: FixedDiffEndpoint::AuthorizedBaseCommit(&base_commit),
                head: checkpoint(&f, &scope, &record),
                paths: &[],
                output: DiffOutput::Metadata,
            },
        )
        .await
        .expect("SHA-256 固定端点比较");
    assert_eq!(snapshot(&f.root), before, "固定端点比较必须零写入");
    assert!(result.is_complete(), "{:?}", result.incomplete_reason);
    let renamed = change(&result, "new-name.txt");
    assert_eq!(renamed.kind, FixedDiffKind::Renamed);
    assert_eq!(
        renamed.old_path.as_deref(),
        Some(b"old-name.txt".as_slice())
    );
    let binary = change(&result, "bin.dat");
    assert!(binary.binary);
    assert_eq!(binary.additions, None);
    assert_eq!(binary.deletions, None);
    let modified = change(&result, "changed.txt");
    assert_eq!(modified.additions, Some(1));
    assert_eq!(modified.deletions, Some(1));

    let review = probe()
        .readonly_fixed_diff(
            &f.root,
            FixedDiffRequest {
                base: FixedDiffEndpoint::AuthorizedBaseCommit(&base_commit),
                head: checkpoint(&f, &scope, &record),
                paths: &[b"bin.dat" as &[u8]],
                output: DiffOutput::Review(b"bin.dat"),
            },
        )
        .await
        .expect("二进制审阅");
    assert_eq!(review.review, Some(DiffReview::Binary));

    let unchanged = probe()
        .readonly_fixed_diff(
            &f.root,
            FixedDiffRequest {
                base: FixedDiffEndpoint::AuthorizedBaseCommit(&base_commit),
                head: checkpoint(&f, &scope, &record),
                paths: &[b"keep.txt" as &[u8]],
                output: DiffOutput::Review(b"keep.txt"),
            },
        )
        .await
        .expect("未变更审阅");
    assert_eq!(unchanged.review, Some(DiffReview::Unchanged));
    assert_eq!(snapshot(&f.root), before, "审阅也必须零写入");
}

#[tokio::test]
async fn linked_worktree_and_unborn_repository_are_supported() {
    let f = Fixture::new("sha1");
    f.write("note.txt", b"base");
    f.commit();
    git(
        &f.root,
        &["worktree", "add", "-q", "../linked", "-b", "linked"],
    );
    let linked = f.dir.join("linked");
    fs::write(linked.join("linked.txt"), b"linked").expect("写入链接工作树");
    let scope = CheckpointScope::new();
    let linked_record = probe()
        .capture_checkpoint(
            &linked,
            &CheckpointOptions {
                state_directory: f.state.clone(),
                scope: scope.clone(),
                turn_index: 1,
                policy: CheckpointPolicy::default(),
            },
        )
        .await
        .expect("链接工作树捕获");
    let linked_base = git_oid(&linked, "HEAD");
    let result = probe()
        .readonly_fixed_diff(
            &linked,
            FixedDiffRequest {
                base: FixedDiffEndpoint::AuthorizedBaseCommit(&linked_base),
                head: FixedDiffEndpoint::AuthorizedCheckpoint {
                    state_directory: f.state.as_path(),
                    scope: &scope,
                    checkpoint_id: linked_record.checkpoint_id.as_str(),
                },
                paths: &[],
                output: DiffOutput::Metadata,
            },
        )
        .await
        .expect("链接工作树固定端点比较");
    assert_eq!(change(&result, "linked.txt").kind, FixedDiffKind::Added);
    assert_ne!(result.base.worktree.git_dir, f.root.join(".git"));

    let unborn = Fixture::new("sha1");
    unborn.write("first.txt", b"one");
    assert!(
        command(&unborn.root, &["rev-parse", "--verify", "HEAD"])
            .status
            .code()
            != Some(0)
    );
    let unborn_scope = CheckpointScope::new();
    let unborn_record = capture(&unborn, &unborn_scope, 1).await;
    let tree = String::from_utf8(git(
        &unborn.root,
        &["hash-object", "-w", "-t", "tree", "--stdin"],
    ))
    .expect("空树文本")
    .trim()
    .to_owned();
    let dangling = String::from_utf8(git(&unborn.root, &["commit-tree", &tree, "-m", "dangling"]))
        .expect("悬挂提交文本")
        .trim()
        .to_owned();
    let error = probe()
        .readonly_fixed_diff(
            &unborn.root,
            FixedDiffRequest {
                base: FixedDiffEndpoint::AuthorizedBaseCommit(&dangling),
                head: FixedDiffEndpoint::AuthorizedCheckpoint {
                    state_directory: unborn.state.as_path(),
                    scope: &unborn_scope,
                    checkpoint_id: unborn_record.checkpoint_id.as_str(),
                },
                paths: &[],
                output: DiffOutput::Metadata,
            },
        )
        .await
        .expect_err("悬挂提交不可达");
    assert!(
        matches!(error, FixedDiffError::UnreachableCommit),
        "{error:?}"
    );

    let result = probe()
        .readonly_fixed_diff(
            &unborn.root,
            FixedDiffRequest {
                base: FixedDiffEndpoint::AuthorizedCheckpoint {
                    state_directory: unborn.state.as_path(),
                    scope: &unborn_scope,
                    checkpoint_id: unborn_record.checkpoint_id.as_str(),
                },
                head: FixedDiffEndpoint::AuthorizedCheckpoint {
                    state_directory: unborn.state.as_path(),
                    scope: &unborn_scope,
                    checkpoint_id: unborn_record.checkpoint_id.as_str(),
                },
                paths: &[],
                output: DiffOutput::Metadata,
            },
        )
        .await
        .expect("unborn 检查点自比较");
    assert!(result.changes.is_empty());
    assert!(result.is_complete());
}
