use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn run(&self, directory: &Path, arguments: &[&str]) -> Output {
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
            ])
            .args(arguments);
        command.output().expect("启动夹具 Git")
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
        self.git(directory, &["commit", "-m", "fixture"]);
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
async fn disables_fsmonitor_and_rejects_external_clean_filters() {
    let fixture = Fixture::new();
    let repository = fixture.repo("unsafe-config");
    fs::write(repository.join("file"), "base\n").expect("base");
    fixture.commit(&repository);
    fixture.git(&repository, &["config", "core.fsmonitor", "does-not-exist"]);
    let before = file_bytes(&fixture.root);
    fixture
        .git
        .probe(&repository)
        .await
        .expect("不调用 fsmonitor");
    assert_eq!(before, file_bytes(&fixture.root));
    fixture.git(
        &repository,
        &["config", "filter.fixture.clean", "does-not-exist"],
    );
    let before = file_bytes(&fixture.root);
    assert!(matches!(
        fixture.git.probe(&repository).await,
        Err(GitError::ExternalFilter)
    ));
    assert_eq!(before, file_bytes(&fixture.root));
}

#[cfg(unix)]
#[tokio::test]
async fn preserves_non_utf8_and_newline_repository_paths() {
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
