use soxu_magpie::{
    EnvironmentMode, WorktreeContext, WorktreeContextError, WorktreeDirectory,
    prepare_worktree_context,
};
#[cfg(any(windows, target_os = "linux"))]
use soxu_magpie::{WORKTREE_CONTEXT_FILE, WorktreeContextServer};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "soxu-context-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).expect("创建夹具文件或目录");
        // `WorktreeDirectory::open` 拒绝含链接的祖先，Unix 上的系统临时目录可能是
        // 别名，因此夹具根创建后再规范化，后续一律使用同一真实根；Windows 保留原路径。
        #[cfg(unix)]
        let path = path.canonicalize().expect("规范化夹具根");
        fs::create_dir(path.join("worktree")).expect("创建夹具文件或目录");
        fs::write(path.join("worktree/.git"), b"gitdir: fixture").expect("创建夹具文件或目录");
        fs::create_dir(path.join("magpie")).expect("创建夹具文件或目录");
        Self(path)
    }

    fn config(&self, data: &str) -> PathBuf {
        let dir = self.0.join("magpie");
        fs::write(dir.join("library.json"), data).expect("创建夹具文件或目录");
        dir
    }

    fn root(&self) -> WorktreeDirectory {
        WorktreeDirectory::open(&self.0.join("worktree")).expect("创建夹具文件或目录")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn native_and_shared_routes_do_not_create_assets() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let config = fixture
        .config(r#"{"mcpHub":true,"mcp":[{"name":"aoci"},{"name":"codegraph","local":false}]}"#);
    assert_eq!(
        prepare_worktree_context(&root, EnvironmentMode::StandaloneNative, None)
            .expect("准备工作区上下文"),
        WorktreeContext::StandaloneNative
    );
    assert_eq!(
        prepare_worktree_context(&root, EnvironmentMode::MagpieManaged, Some(&config))
            .expect("准备工作区上下文"),
        WorktreeContext::NoDirectServers
    );
    assert_eq!(
        fs::read_dir(root.path())
            .expect("读取夹具文件或目录")
            .count(),
        1
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn direct_context_is_root_relative_and_idempotent() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let source = r#"{"mcpHub":false,"mcp":[{"name":"AOCI","env":{"TEST_SECRET":"fixture-secret"}},{"name":"codegraph","local":false},{"name":"aoci"},{"name":"unrelated"}]}"#;
    let config = fixture.config(source);
    let manifest = root.path().join(WORKTREE_CONTEXT_FILE);
    for _ in 0..2 {
        assert_eq!(
            prepare_worktree_context(&root, EnvironmentMode::MagpieManaged, Some(&config))
                .expect("准备工作区上下文"),
            WorktreeContext::Prepared {
                manifest: manifest.clone(),
                servers: vec![
                    WorktreeContextServer::Aoci,
                    WorktreeContextServer::CodeGraph
                ],
            }
        );
    }
    let data: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).expect("读取夹具文件或目录"))
            .expect("读取夹具文件或目录");
    assert_eq!(
        data,
        serde_json::json!({"version":1,"worktree_root":".","servers":["aoci","code_graph"]})
    );
    assert_eq!(
        fs::read_to_string(config.join("library.json")).expect("读取夹具文件或目录"),
        source
    );
    assert_eq!(
        fs::read_dir(root.path())
            .expect("读取夹具文件或目录")
            .count(),
        2
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn existing_asset_is_preserved_and_a_later_request_can_retry() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let config = fixture
        .config(r#"{"mcpHub":true,"mcp":[{"name":"aoci","local":true},{"name":"codegraph"}]}"#);
    let manifest = root.path().join(WORKTREE_CONTEXT_FILE);
    fs::write(&manifest, b"user-owned").expect("创建夹具文件或目录");
    assert!(matches!(
        prepare_worktree_context(&root, EnvironmentMode::MagpieManaged, Some(&config)),
        Err(WorktreeContextError::Conflict)
    ));
    assert_eq!(
        fs::read(&manifest).expect("读取夹具文件或目录"),
        b"user-owned"
    );
    fs::remove_file(&manifest).expect("更新夹具文件");
    assert!(
        matches!(prepare_worktree_context(&root, EnvironmentMode::MagpieManaged, Some(&config)).expect("准备工作区上下文"), WorktreeContext::Prepared { servers, .. } if servers == [WorktreeContextServer::Aoci])
    );
}

#[cfg(any(unix, windows))]
#[test]
fn rejects_a_link_in_the_authorized_directory_chain() {
    let fixture = Fixture::new();
    let outside = fixture.0.join("outside");
    fs::create_dir_all(outside.join("child")).expect("创建夹具文件或目录");
    let link = fixture.0.join("link");
    create_directory_link(&outside, &link);
    assert!(matches!(
        WorktreeDirectory::open(&link.join("child")),
        Err(WorktreeContextError::UnsafeDirectory)
    ));
    assert!(matches!(
        WorktreeDirectory::open(&link),
        Err(WorktreeContextError::UnsafeDirectory)
    ));
    assert_eq!(
        fs::read_dir(outside.join("child"))
            .expect("读取夹具文件或目录")
            .count(),
        0
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_link_at_the_manifest_path_is_rejected() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let outside = fixture.0.join("outside");
    fs::create_dir(&outside).expect("创建夹具文件或目录");
    let manifest = root.path().join(WORKTREE_CONTEXT_FILE);
    create_directory_link(&outside, &manifest);
    let config = fixture.config(r#"{"mcpHub":false,"mcp":[{"name":"aoci"}]}"#);
    assert!(matches!(
        prepare_worktree_context(&root, EnvironmentMode::MagpieManaged, Some(&config)),
        Err(WorktreeContextError::Conflict)
    ));
    assert_eq!(
        fs::read_dir(outside).expect("读取夹具文件或目录").count(),
        0
    );
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("读取夹具文件或目录");
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) {
    let output = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .args([link, target])
        .output()
        .expect("运行夹具子进程");
    assert!(output.status.success(), "无法创建临时目录联接");
}

#[cfg(target_os = "linux")]
#[test]
fn a_replaced_root_cannot_receive_a_manifest() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let replacement = fixture.0.join("replacement");
    fs::rename(root.path(), &replacement).expect("更新夹具文件");
    fs::create_dir(root.path()).expect("创建夹具文件或目录");
    let config = fixture.config(r#"{"mcpHub":false,"mcp":[{"name":"aoci"}]}"#);
    assert!(matches!(
        prepare_worktree_context(&root, EnvironmentMode::MagpieManaged, Some(&config)),
        Err(WorktreeContextError::UnsafeDirectory)
    ));
    assert_eq!(
        fs::read_dir(root.path())
            .expect("读取夹具文件或目录")
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(replacement)
            .expect("读取夹具文件或目录")
            .count(),
        1
    );
}
