use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use soxu_git::{
    GitError, GitProbe, WorktreeHead, WorktreeInventoryError, WorktreeKind, parse_worktrees,
};

fn record(path: &[u8], oid: &[u8], state: &[u8], annotations: &[u8]) -> Vec<u8> {
    let mut bytes = b"worktree ".to_vec();
    bytes.extend_from_slice(path);
    bytes.extend_from_slice(b"\0HEAD ");
    bytes.extend_from_slice(oid);
    bytes.push(0);
    bytes.extend_from_slice(state);
    bytes.push(0);
    bytes.extend_from_slice(annotations);
    bytes.push(0);
    bytes
}

#[test]
fn preserves_nul_format_bytes_and_annotations_without_unquoting() {
    let input = record(
        ["/repo space/索序\n\"".as_bytes(), b"\xff"]
            .concat()
            .as_slice(),
        &[b'a'; 40],
        "branch refs/heads/索序".as_bytes(),
        b"locked line one
line two\xff\0prunable \0",
    );
    let inventory = parse_worktrees(&input).expect("夹具或解析结果必须有效");
    let entry = &inventory.entries[0];
    assert_eq!(entry.kind, WorktreeKind::Main);
    assert_eq!(
        entry.path.as_bytes(),
        ["/repo space/索序\n\"".as_bytes(), b"\xff"]
            .concat()
            .as_slice()
    );
    assert_eq!(
        entry
            .locked
            .as_ref()
            .expect("夹具或解析结果必须有效")
            .reason
            .as_deref(),
        Some(
            b"line one
line two\xff"
                .as_slice()
        )
    );
    assert_eq!(
        entry
            .prunable
            .as_ref()
            .expect("夹具或解析结果必须有效")
            .reason
            .as_deref(),
        Some(b"".as_slice())
    );
    assert!(
        matches!(&entry.head, WorktreeHead::Attached { branch, .. } if branch == "refs/heads/索序".as_bytes())
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            entry
                .path
                .to_path_buf()
                .expect("夹具或解析结果必须有效")
                .as_os_str()
                .as_bytes(),
            entry.path.as_bytes()
        );
    }
    #[cfg(not(unix))]
    assert!(entry.path.to_path_buf().is_err());
}

#[test]
fn parses_main_linked_bare_unborn_detached_and_reasonless_flags() {
    let mut input = b"worktree /bare\0bare\0\0".to_vec();
    input.extend(record(
        b"/new",
        &[b'0'; 64],
        b"branch refs/heads/new",
        b"locked\0prunable\0",
    ));
    input.extend(record(b"/detached", &[b'B'; 64], b"detached", b""));
    let entries = parse_worktrees(&input)
        .expect("夹具或解析结果必须有效")
        .entries;
    assert_eq!(entries[0].head, WorktreeHead::Bare);
    assert_eq!(entries[1].kind, WorktreeKind::Linked);
    assert!(matches!(&entries[1].head, WorktreeHead::Unborn { oid, .. } if oid == &"0".repeat(64)));
    assert_eq!(
        entries[1]
            .locked
            .as_ref()
            .expect("夹具或解析结果必须有效")
            .reason,
        None
    );
    assert_eq!(
        entries[1]
            .prunable
            .as_ref()
            .expect("夹具或解析结果必须有效")
            .reason,
        None
    );
    assert!(matches!(&entries[2].head, WorktreeHead::Detached { oid } if oid == &"B".repeat(64)));
}

#[test]
fn rejects_truncation_duplicates_conflicts_and_unknown_future_fields_atomically() {
    let valid = record(b"/repo", &[b'a'; 40], b"branch refs/heads/main", b"");
    for end in 0..valid.len() {
        assert!(parse_worktrees(&valid[..end]).is_err(), "截断 {end}");
    }
    let cases = [
        b"worktree /x\0bare\0bare\0\0".to_vec(),
        b"worktree /x\0bare true\0\0".to_vec(),
        b"worktree /x\0bare\0HEAD a\0\0".to_vec(),
        b"worktree /x\0bare\0branch refs/heads/x\0\0".to_vec(),
        b"worktree /x\0bare\0detached\0\0".to_vec(),
        b"worktree /x\0bare\0future value\0\0".to_vec(),
        b"worktree /x\0bare\0worktree /y\0\0".to_vec(),
        b"HEAD x\0worktree /x\0\0".to_vec(),
        b"worktree relative\0bare\0\0".to_vec(),
        b"worktree \0bare\0\0".to_vec(),
        b"worktree /x\0\0".to_vec(),
        b"\0".to_vec(),
        record(b"/x", &[b'g'; 40], b"detached", b""),
        record(b"/x", &[b'a'; 39], b"detached", b""),
        record(b"/x", &[b'a'; 40], b"branch refs/heads/", b""),
        record(b"/x", &[b'a'; 40], b"branch main", b""),
        record(b"/x", &[b'a'; 40], b"branch refs/heads/x", b"detached\0"),
        record(b"/x", &[b'a'; 40], b"detached", b"HEAD a\0"),
        record(b"/x", &[b'a'; 40], b"detached", b"locked\0locked reason\0"),
        record(
            b"/x",
            &[b'a'; 40],
            b"detached",
            b"prunable reason\0prunable\0",
        ),
        record(
            b"/x",
            &[b'a'; 40],
            b"branch refs/heads/x",
            b"branch refs/heads/y\0",
        ),
        record(b"/x", &[b'a'; 40], b"detached", b"detached\0"),
    ];
    for bad in cases {
        assert!(parse_worktrees(&bad).is_err(), "非法单条 {bad:?}");
        let mut batch = valid.clone();
        batch.extend(&bad);
        assert!(parse_worktrees(&batch).is_err(), "不得返回半份清单 {bad:?}");
    }
    let mut repeated = valid.clone();
    repeated.extend(&valid);
    assert!(parse_worktrees(&repeated).is_err());
    let mut mixed = valid.clone();
    mixed.extend(record(
        b"/sha256",
        &[b'0'; 64],
        b"branch refs/heads/new",
        b"",
    ));
    assert!(parse_worktrees(&mixed).is_err());
    let mut trailing = valid;
    trailing.push(0);
    assert!(parse_worktrees(&trailing).is_err());
}

#[test]
fn path_conversion_is_lossless_or_explicitly_rejected() {
    let path = if cfg!(windows) {
        "C:/临时 目录/repo"
    } else {
        "/临时 目录/repo"
    };
    let inventory = parse_worktrees(&record(path.as_bytes(), &[b'a'; 40], b"detached", b""))
        .expect("夹具或解析结果必须有效");
    assert_eq!(
        inventory.entries[0]
            .path
            .to_path_buf()
            .expect("夹具或解析结果必须有效"),
        PathBuf::from(path)
    );
    for path in [
        b"/raw-\xff".as_slice(),
        b"C:/CON/file",
        b"C:/repo./file",
        b"C:/LPT1/file",
        b"C:/repo /file",
        b"C:/repo:other/file",
        b"C:/repo\\file",
    ] {
        let entries = parse_worktrees(&record(path, &[b'a'; 40], b"detached", b""))
            .expect("夹具或解析结果必须有效")
            .entries;
        assert_eq!(entries[0].path.as_bytes(), path);
        #[cfg(not(unix))]
        assert!(entries[0].path.to_path_buf().is_err());
    }
}

#[test]
fn rejects_invalid_git_refs_and_noncanonical_paths_but_preserves_native_bytes() {
    for branch in [
        b"refs/heads/a b".as_slice(),
        b"refs/heads/a\nb",
        b"refs/heads/a..b",
        b"refs/heads/a@{b",
        b"refs/heads/a.lock",
        b"refs/heads/a.lock/b",
        b"refs/heads/.hidden",
        b"refs/heads/a//b",
        b"refs/heads/a/",
        b"refs/heads/a.",
        b"refs/heads/a~b",
        b"refs/heads/a^b",
        b"refs/heads/a:b",
        b"refs/heads/a?b",
        b"refs/heads/a*b",
        b"refs/heads/a[b",
        b"refs/heads/a\\b",
        b"refs/heads/a\x7fb",
        b"refs/heads/a\x01b",
    ] {
        let mut state = b"branch ".to_vec();
        state.extend(branch);
        assert!(parse_worktrees(&record(b"/repo", &[b'a'; 40], &state, b"")).is_err());
    }
    for path in [
        b"/repo/../x".as_slice(),
        b"/repo/./x",
        b"/repo//x",
        b"/repo/",
        b"///repo",
        b"//server",
        b"C:/repo/../x",
        b"C:/repo//x",
        b"C:repo",
    ] {
        assert!(parse_worktrees(&record(path, &[b'a'; 40], b"detached", b"")).is_err());
    }
    for path in [
        b"/".as_slice(),
        b"C:/",
        b"//server/share",
        b"/repo\nline",
        b"/repo-\xff",
    ] {
        assert!(parse_worktrees(&record(path, &[b'a'; 40], b"detached", b"")).is_ok());
    }
    for branch in [
        b"refs/heads/nonutf-\xff".as_slice(),
        b"refs/heads/a@b",
        b"refs/heads/a.LOCK",
        b"refs/heads/a./b",
    ] {
        let mut state = b"branch ".to_vec();
        state.extend(branch);
        let inventory =
            parse_worktrees(&record(b"/repo", &[b'a'; 40], &state, b"")).expect("合法原生 ref");
        assert!(
            matches!(&inventory.entries[0].head, WorktreeHead::Attached { branch: actual, .. } if actual == branch)
        );
    }
}

struct Fixture {
    root: PathBuf,
    git: GitProbe,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let temp = std::env::temp_dir()
            .canonicalize()
            .expect("夹具或解析结果必须有效");
        let root = loop {
            let root = temp.join(format!(
                "soxu-worktrees-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => break root,
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("创建临时夹具失败：{error}"),
            }
        };
        Self {
            root,
            git: GitProbe::discover().expect("需要原生 Git"),
        }
    }

    // L-014：与生产只读入口同源禁用后台维护。
    fn run(&self, directory: &Path, arguments: &[&str]) -> Vec<u8> {
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
        let output = command
            .current_dir(directory)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("LC_ALL", "C")
            .args([
                "-c",
                "core.hooksPath=",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ])
            .args(arguments)
            .output()
            .expect("夹具或解析结果必须有效");
        assert!(
            output.status.success(),
            "夹具 Git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    fn repo(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir(&path).expect("夹具或解析结果必须有效");
        self.run(&path, &["init", "--initial-branch=main"]);
        path
    }

    fn snapshot(&self) -> BTreeMap<PathBuf, (SystemTime, Option<Vec<u8>>)> {
        let mut result = BTreeMap::new();
        let mut pending = vec![self.root.clone()];
        while let Some(path) = pending.pop() {
            let metadata = path.symlink_metadata().expect("夹具或解析结果必须有效");
            assert!(!metadata.is_symlink(), "夹具不得跟随外部链接");
            let bytes = if metadata.is_dir() {
                pending.extend(
                    fs::read_dir(&path)
                        .expect("夹具或解析结果必须有效")
                        .map(|error| error.expect("夹具或解析结果必须有效").path()),
                );
                None
            } else {
                Some(fs::read(&path).expect("夹具或解析结果必须有效"))
            };
            result.insert(
                path,
                (metadata.modified().expect("夹具或解析结果必须有效"), bytes),
            );
        }
        result
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(
            self.root.parent(),
            Some(
                std::env::temp_dir()
                    .canonicalize()
                    .expect("夹具或解析结果必须有效")
                    .as_path()
            )
        );
        assert!(
            self.root
                .file_name()
                .expect("夹具或解析结果必须有效")
                .to_string_lossy()
                .starts_with("soxu-worktrees-")
        );
        fs::remove_dir_all(&self.root).expect("清理临时夹具");
    }
}

#[test]
fn branch_validation_matches_native_git_check_ref_format() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    for branch in [
        "refs/heads/main",
        "refs/heads/索序",
        "refs/heads/a@b",
        "refs/heads/a.LOCK",
        "refs/heads/a./b",
        "refs/heads/a b",
        "refs/heads/a\nb",
        "refs/heads/a..b",
        "refs/heads/a@{b",
        "refs/heads/a.lock",
        "refs/heads/a.lock/b",
        "refs/heads/.hidden",
        "refs/heads/a//b",
        "refs/heads/a/",
        "refs/heads/a.",
        "refs/heads/a~b",
        "refs/heads/a^b",
        "refs/heads/a:b",
        "refs/heads/a?b",
        "refs/heads/a*b",
        "refs/heads/a[b",
        "refs/heads/a\\b",
        "refs/heads/a\u{7f}b",
    ] {
        let native = Command::new(fixture.git.program())
            .current_dir(&fixture.root)
            .args(["check-ref-format", branch])
            .output()
            .expect("运行原生 Git ref 格式校验");
        let mut state = b"branch ".to_vec();
        state.extend(branch.as_bytes());
        assert_eq!(
            parse_worktrees(&record(b"/repo", &[b'a'; 40], &state, b"")).is_ok(),
            native.status.success(),
            "ref {branch:?}"
        );
    }
    assert_eq!(before, fixture.snapshot());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn native_linux_newline_and_non_utf8_worktree_paths_remain_lossless() {
    use std::os::unix::ffi::OsStrExt;
    let fixture = Fixture::new();
    let repo = fixture.repo("main");
    fixture.run(&repo, &["commit", "--allow-empty", "-m", "fixture"]);
    fixture.run(&repo, &["worktree", "add", "--detach", "../line\nbreak"]);
    let raw_path = fixture.root.join(std::ffi::OsStr::from_bytes(b"raw-\xff"));
    fs::create_dir(&raw_path).expect("创建非 UTF-8 临时夹具路径");
    fixture.run(&raw_path, &["init", "--initial-branch=main"]);
    let before = fixture.snapshot();
    let inventory = fixture
        .git
        .list_worktrees(&repo)
        .await
        .expect("读取换行夹具清单");
    assert!(
        inventory
            .entries
            .iter()
            .any(|entry| entry.path.as_bytes().ends_with(b"/line\nbreak"))
    );
    let raw_inventory = fixture
        .git
        .list_worktrees(&raw_path)
        .await
        .expect("读取非 UTF-8 夹具清单");
    assert_eq!(
        raw_inventory.entries[0].path.as_bytes(),
        raw_path.as_os_str().as_bytes()
    );
    assert_eq!(before, fixture.snapshot());
}

#[tokio::test]
async fn native_unborn_and_input_errors_leave_fixture_unchanged() {
    let fixture = Fixture::new();
    let repo = fixture.repo("unborn 空格 索序");
    let plain = fixture.root.join("plain");
    fs::create_dir(&plain).expect("夹具或解析结果必须有效");
    let file = plain.join("file");
    fs::write(&file, b"fixture").expect("夹具或解析结果必须有效");
    let before = fixture.snapshot();
    let inventory = fixture
        .git
        .list_worktrees(&repo)
        .await
        .expect("夹具或解析结果必须有效");
    assert_eq!(inventory.entries.len(), 1);
    assert!(
        matches!(&inventory.entries[0].head, WorktreeHead::Unborn { oid, branch } if oid == &"0".repeat(40) && branch == b"refs/heads/main")
    );
    assert_eq!(
        inventory.entries[0]
            .path
            .to_path_buf()
            .expect("夹具或解析结果必须有效")
            .canonicalize()
            .expect("夹具或解析结果必须有效"),
        repo.canonicalize().expect("夹具或解析结果必须有效")
    );
    assert!(matches!(
        fixture.git.list_worktrees("relative").await,
        Err(WorktreeInventoryError::Git(GitError::RelativePath))
    ));
    assert!(matches!(
        fixture.git.list_worktrees(&file).await,
        Err(WorktreeInventoryError::Git(GitError::NotDirectory))
    ));
    assert!(matches!(
        fixture.git.list_worktrees(&plain).await,
        Err(WorktreeInventoryError::Git(GitError::CommandFailed {
            operation: "worktree",
            code: Some(128),
            ..
        }))
    ));
    assert_eq!(before, fixture.snapshot());
}

#[tokio::test]
async fn native_main_linked_detached_locked_and_prunable_are_registration_facts() {
    let fixture = Fixture::new();
    let repo = fixture.repo("main 空格 索序");
    fixture.run(&repo, &["commit", "--allow-empty", "-m", "fixture"]);
    let linked = fixture.root.join("linked 空格 索序");
    let detached = fixture.root.join("detached");
    let missing = fixture.root.join("missing");
    fixture.run(
        &repo,
        &["worktree", "add", "-b", "topic", "../linked 空格 索序"],
    );
    fixture.run(&repo, &["worktree", "add", "--detach", "../detached"]);
    fixture.run(&repo, &["worktree", "add", "--detach", "../missing"]);
    let reason = "便携 设备
第二行 \"引号\"";
    fixture.run(
        &repo,
        &[
            "worktree",
            "lock",
            "--reason",
            reason,
            "../linked 空格 索序",
        ],
    );
    fixture.run(&repo, &["worktree", "lock", "../detached"]);
    fs::rename(&missing, fixture.root.join("moved")).expect("夹具或解析结果必须有效");
    let before = fixture.snapshot();
    let from_main = fixture
        .git
        .list_worktrees(&repo)
        .await
        .expect("夹具或解析结果必须有效");
    let from_linked = fixture
        .git
        .list_worktrees(&linked)
        .await
        .expect("夹具或解析结果必须有效");
    assert_eq!(from_main, from_linked);
    assert_eq!(from_main.entries.len(), 4);
    assert_eq!(from_main.entries[0].kind, WorktreeKind::Main);
    assert!(
        matches!(&from_main.entries[0].head, WorktreeHead::Attached { branch, oid } if branch == b"refs/heads/main" && oid.len() == 40)
    );
    let registered_parent = from_main.entries[0]
        .path
        .to_path_buf()
        .expect("主工作树原生路径");
    let registered_parent = registered_parent
        .parent()
        .expect("夹具主工作树位于临时根内");
    let find = |path: &Path| {
        let expected = registered_parent.join(path.file_name().expect("夹具工作树名称"));
        from_main
            .entries
            .iter()
            .find(|entry| entry.path.to_path_buf().expect("注册工作树原生路径") == expected)
            .expect("夹具或解析结果必须有效")
    };
    let linked_entry = find(&linked);
    assert_eq!(linked_entry.kind, WorktreeKind::Linked);
    assert!(
        matches!(&linked_entry.head, WorktreeHead::Attached { branch, .. } if branch == b"refs/heads/topic")
    );
    assert_eq!(
        linked_entry
            .locked
            .as_ref()
            .expect("夹具或解析结果必须有效")
            .reason
            .as_deref(),
        Some(reason.as_bytes())
    );
    assert!(matches!(
        &find(&detached).head,
        WorktreeHead::Detached { .. }
    ));
    assert_eq!(
        find(&detached)
            .locked
            .as_ref()
            .expect("夹具或解析结果必须有效")
            .reason,
        None
    );
    let reason = find(&missing)
        .prunable
        .as_ref()
        .expect("夹具或解析结果必须有效")
        .reason
        .as_deref()
        .expect("夹具或解析结果必须有效");
    assert!(reason.starts_with(b"gitdir file points to non-existent location"));
    assert_eq!(before, fixture.snapshot());
}

#[tokio::test]
async fn native_linked_missing_head_is_reported_as_unavailable_and_restored() {
    let fixture = Fixture::new();
    let repo = fixture.repo("main");
    fixture.run(&repo, &["commit", "--allow-empty", "-m", "fixture"]);
    fixture.run(&repo, &["worktree", "add", "--detach", "../linked"]);
    let linked = fixture.root.join("linked");
    let project = fixture
        .git
        .open_local_project(&linked)
        .await
        .expect("读取临时 linked 身份");
    let soxu_git::LocalProject::GitWorktree(identity) = project else {
        panic!("夹具必须是 linked 工作树");
    };
    let head_path = identity.git_dir.join("HEAD");
    let saved_head = fixture.root.join("saved-head");
    let original = fixture.snapshot();
    fs::rename(&head_path, &saved_head).expect("仅移动自建夹具 HEAD 并保留恢复文件");
    let before_probe = fixture.snapshot();
    let native_output = fixture.run(&repo, &["worktree", "list", "--porcelain", "-z"]);
    let inventory_result = fixture.git.list_worktrees(&repo).await;
    let after_probe = fixture.snapshot();
    fs::rename(&saved_head, &head_path).expect("恢复自建夹具 HEAD");
    let restored = fixture.snapshot();
    assert_eq!(
        original.keys().collect::<Vec<_>>(),
        restored.keys().collect::<Vec<_>>()
    );
    for (path, (_, bytes)) in original {
        assert_eq!(bytes, restored[&path].1, "恢复夹具文件字节");
    }
    assert_eq!(before_probe, after_probe);
    let inventory = inventory_result.expect("缺失 HEAD 的原生清单仍可表达");
    assert_eq!(
        inventory,
        parse_worktrees(&native_output).expect("解析缺失 HEAD 的真实 porcelain")
    );
    assert_eq!(inventory.entries.len(), 2);
    assert!(
        matches!(&inventory.entries[1].head, WorktreeHead::Unavailable { oid, detached: true } if oid == &"0".repeat(40))
    );
}

#[tokio::test]
async fn native_bare_main_can_be_reported_without_changing_project_open_contract() {
    let fixture = Fixture::new();
    let bare = fixture.root.join("bare");
    fs::create_dir(&bare).expect("夹具或解析结果必须有效");
    fixture.run(&bare, &["init", "--bare"]);
    let before = fixture.snapshot();
    let inventory = fixture
        .git
        .list_worktrees(&bare)
        .await
        .expect("夹具或解析结果必须有效");
    assert_eq!(inventory.entries.len(), 1);
    assert_eq!(inventory.entries[0].head, WorktreeHead::Bare);
    assert_eq!(inventory.entries[0].kind, WorktreeKind::Main);
    assert_eq!(before, fixture.snapshot());
}
