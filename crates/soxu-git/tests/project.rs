use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use soxu_git::{GitError, GitProbe, LocalProject, ProjectOpenError, WorktreeIdentity};

struct Fixture {
    root: PathBuf,
    git: GitProbe,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let temp = std::env::temp_dir().canonicalize().expect("临时目录");
        let root = loop {
            let root = temp.join(format!(
                "soxu-project-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => break root,
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("创建夹具失败：{error}"),
            }
        };
        Self {
            root,
            git: GitProbe::discover().expect("需要原生 Git"),
        }
    }

    fn directory(&self, path: &str) -> PathBuf {
        let path = self.root.join(path);
        fs::create_dir_all(&path).expect("创建夹具目录");
        path
    }

    // L-014：与生产只读入口同源禁用后台维护。
    fn run(&self, directory: &Path, arguments: &[&str]) {
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
            .expect("运行夹具 Git");
        assert!(
            output.status.success(),
            "Git {arguments:?}：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repo(&self, path: &str) -> PathBuf {
        let directory = self.directory(path);
        self.run(&directory, &["init", "--initial-branch=main"]);
        directory
    }

    fn snapshot(&self) -> BTreeMap<PathBuf, (SystemTime, Option<Vec<u8>>)> {
        let mut tree = BTreeMap::new();
        let mut pending = vec![self.root.clone()];
        while let Some(path) = pending.pop() {
            let metadata = path.metadata().expect("夹具元数据");
            let bytes = if metadata.is_dir() {
                pending.extend(
                    path.read_dir()
                        .expect("枚举夹具")
                        .map(|entry| entry.expect("夹具目录项").path()),
                );
                None
            } else {
                Some(fs::read(&path).expect("夹具文件"))
            };
            tree.insert(path, (metadata.modified().expect("修改时间"), bytes));
        }
        tree
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(
            self.root.parent(),
            Some(
                std::env::temp_dir()
                    .canonicalize()
                    .expect("临时目录")
                    .as_path()
            )
        );
        assert!(
            self.root
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("soxu-project-"))
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn opens_plain_directory_and_rejects_invalid_input_without_writing() {
    let fixture = Fixture::new();
    let plain = fixture.directory("普通 目录");
    fixture.directory("普通 目录/objects");
    fixture.directory("普通 目录/refs");
    fixture.directory("普通 目录/config");
    let file = plain.join("file");
    fs::write(&file, "fixture").expect("文件夹具");
    let before = fixture.snapshot();
    assert_eq!(
        fixture
            .git
            .open_local_project(plain.join("."))
            .await
            .expect("普通目录"),
        LocalProject::Directory { root: plain }
    );
    assert!(matches!(
        fixture.git.open_local_project("relative").await,
        Err(ProjectOpenError::Git(GitError::RelativePath))
    ));
    assert!(matches!(
        fixture.git.open_local_project(fixture.root.join("missing")).await,
        Err(ProjectOpenError::Git(GitError::Io(error))) if error.kind() == ErrorKind::NotFound
    ));
    assert!(matches!(
        fixture.git.open_local_project(file).await,
        Err(ProjectOpenError::Git(GitError::NotDirectory))
    ));
    assert_eq!(before, fixture.snapshot());
}

#[tokio::test]
async fn returns_each_worktree_root_and_the_shared_common_dir() {
    let fixture = Fixture::new();
    let repo = fixture.repo("仓库 with space");
    let git_dir = repo.join(".git").canonicalize().expect("Git 目录");
    let main = LocalProject::GitWorktree(WorktreeIdentity {
        root: repo.clone(),
        git_dir: git_dir.clone(),
        common_dir: git_dir.clone(),
    });
    let child = fixture.directory("仓库 with space/main-child");
    let before = fixture.snapshot();
    assert_eq!(
        fixture
            .git
            .open_local_project(child)
            .await
            .expect("未提交工作树"),
        main
    );
    assert_eq!(before, fixture.snapshot());

    fixture.run(&repo, &["commit", "--allow-empty", "-m", "fixture"]);
    fixture.run(
        &repo,
        &["worktree", "add", "-b", "linked", "../另一个 worktree"],
    );
    let linked_child = fixture.directory("另一个 worktree/linked-child");
    let linked = linked_child.parent().expect("linked 根").to_path_buf();
    let mut admins = fs::read_dir(git_dir.join("worktrees")).expect("工作树元数据目录");
    let linked_git_dir = admins
        .next()
        .expect("linked 元数据条目")
        .expect("读取 linked 元数据条目")
        .path()
        .canonicalize()
        .expect("linked Git 目录");
    assert!(admins.next().is_none());
    let linked_identity = LocalProject::GitWorktree(WorktreeIdentity {
        root: linked.clone(),
        git_dir: linked_git_dir.clone(),
        common_dir: git_dir.clone(),
    });
    let before = fixture.snapshot();
    assert_eq!(
        fixture
            .git
            .open_local_project(repo)
            .await
            .expect("主工作树根"),
        main
    );
    assert_eq!(
        fixture
            .git
            .open_local_project(linked)
            .await
            .expect("linked 根"),
        linked_identity
    );
    assert_eq!(
        fixture
            .git
            .open_local_project(linked_child)
            .await
            .expect("linked 子目录"),
        linked_identity
    );
    for metadata in [git_dir.clone(), git_dir.join("objects"), linked_git_dir] {
        assert!(matches!(
            fixture.git.open_local_project(metadata).await,
            Err(ProjectOpenError::GitMetadataDirectory)
        ));
    }
    assert_eq!(before, fixture.snapshot());
}

#[tokio::test]
async fn identity_works_with_a_corrupt_index_and_external_filter() {
    let fixture = Fixture::new();
    let repo = fixture.repo("repo");
    fixture.run(
        &repo,
        &["config", "filter.fixture.process", "sh ../marker.sh"],
    );
    fs::write(fixture.root.join("marker.sh"), "printf ran > ../executed\n").expect("脚本夹具");
    fs::write(repo.join(".git/index"), "bad index contents").expect("损坏索引");
    let before = fixture.snapshot();
    assert!(matches!(
        fixture.git.open_local_project(&repo).await,
        Ok(LocalProject::GitWorktree(identity)) if identity.root == repo
    ));
    assert!(matches!(
        fixture.git.probe(&repo).await,
        Err(GitError::ExternalFilter)
    ));
    assert_eq!(before, fixture.snapshot());

    fixture.run(&repo, &["config", "--unset", "filter.fixture.process"]);
    let before = fixture.snapshot();
    assert!(matches!(
        fixture.git.open_local_project(&repo).await,
        Ok(LocalProject::GitWorktree(_))
    ));
    assert!(matches!(
        fixture.git.probe(&repo).await,
        Err(GitError::CommandFailed {
            operation: "ls-files",
            ..
        })
    ));
    assert_eq!(before, fixture.snapshot());
}

#[tokio::test]
async fn rejects_bare_corrupt_and_shadowing_metadata_without_fallback() {
    let fixture = Fixture::new();
    let bare = fixture.directory("bare");
    fixture.run(&bare, &["init", "--bare"]);
    let bare_child = bare.join("objects");
    let repo = fixture.repo("outer");
    let nested = fixture.directory("outer/inner/subdir");
    let broken_dir = fixture.directory("outer/inner/.git");
    let malformed_gitfile = fixture.directory("malformed");
    fs::write(malformed_gitfile.join(".git"), "malformed gitfile\n").expect("损坏 Git 文件");
    let dangling_gitfile = fixture.directory("dangling");
    fs::write(
        dangling_gitfile.join(".git"),
        "gitdir: ../missing-metadata\n",
    )
    .expect("无效元数据目标");
    let corrupt_config = fixture.repo("corrupt-config");
    fs::write(corrupt_config.join(".git/config"), "[broken\n").expect("损坏配置");
    let corrupt_repo = fixture.repo("corrupt-head");
    fs::write(corrupt_repo.join(".git/HEAD"), "bad HEAD\n").expect("损坏 HEAD");
    let damaged_bare = fixture.directory("damaged-bare");
    fixture.run(&damaged_bare, &["init", "--bare"]);
    fs::rename(damaged_bare.join("HEAD"), damaged_bare.join("HEAD.saved")).expect("移开 bare HEAD");
    let missing_objects = fixture.directory("missing-objects");
    fixture.run(&missing_objects, &["init", "--bare"]);
    fs::rename(
        missing_objects.join("objects"),
        missing_objects.join("objects.saved"),
    )
    .expect("移开对象目录");
    let before = fixture.snapshot();
    for path in [bare, bare_child] {
        let result = fixture.git.open_local_project(path).await;
        assert!(
            matches!(result, Err(ProjectOpenError::BareRepository)),
            "{result:?}"
        );
    }
    for path in [
        nested,
        broken_dir,
        malformed_gitfile,
        dangling_gitfile,
        corrupt_config,
        corrupt_repo,
        damaged_bare,
        missing_objects,
    ] {
        let result = fixture.git.open_local_project(&path).await;
        assert!(
            matches!(result, Err(ProjectOpenError::InvalidRepository { .. })),
            "{path:?}: {result:?}"
        );
    }
    assert!(matches!(
        fixture.git.open_local_project(repo).await,
        Ok(LocalProject::GitWorktree(_))
    ));
    assert_eq!(before, fixture.snapshot());
}

#[cfg(windows)]
#[tokio::test]
async fn rejects_inaccessible_gitfile_without_writing() {
    use std::os::windows::fs::OpenOptionsExt;

    let fixture = Fixture::new();
    let repo = fixture.directory("repo");
    fixture.run(&repo, &["init", "--separate-git-dir", "../metadata"]);
    let metadata = fixture.root.join("metadata");
    let marker = repo.join(".git");
    let before = fixture.snapshot();
    assert_eq!(
        fixture
            .git
            .open_local_project(&repo)
            .await
            .expect("独立元数据工作树"),
        LocalProject::GitWorktree(WorktreeIdentity {
            root: repo.clone(),
            git_dir: metadata.clone(),
            common_dir: metadata,
        })
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&marker)
        .expect("独占 Git 文件");
    assert!(fs::read(&marker).is_err(), "必须实际拒绝读取");
    let result = fixture.git.open_local_project(repo).await;
    drop(lock);
    assert!(
        matches!(
            result,
            Err(ProjectOpenError::Git(
                GitError::Io(_) | GitError::CommandFailed { .. }
            ))
        ),
        "{result:?}"
    );
    assert_eq!(before, fixture.snapshot());
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_unreadable_input_when_permissions_are_enforced() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let directory = fixture.directory("unreadable");
    let permissions = directory.metadata().expect("原始权限").permissions();
    let before = fixture.snapshot();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o000)).expect("限制目录访问");
    let denied = directory.read_dir().is_err();
    let result = fixture.git.open_local_project(&directory).await;
    fs::set_permissions(directory, permissions).expect("恢复夹具权限");
    if denied {
        assert!(matches!(
            result,
            Err(ProjectOpenError::Git(GitError::Io(error))) if error.kind() == ErrorKind::PermissionDenied
        ));
    }
    assert_eq!(before, fixture.snapshot());
}
