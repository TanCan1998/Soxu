use crate::worktree_checkout::sandbox::*;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct Fixture(PathBuf);

impl Fixture {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "soxu-checkout-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path.canonicalize().expect("规范化夹具路径")),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("无法创建夹具：{error}"),
            }
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn git(git: &GitProbe, root: &Path, args: &[&str]) -> String {
    let output = git
        .command(root)
        .args(args)
        .output()
        .await
        .expect("核对夹具操作结果");
    assert!(
        output.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("读取 Git 夹具输出")
        .trim()
        .to_owned()
}

async fn seed(git: &GitProbe, root: &Path) -> String {
    fs::create_dir(root).expect("创建夹具路径");
    git_fixture(git, root, &["init", "--template="]).await;
    fs::create_dir(root.join("nested")).expect("创建夹具路径");
    fs::write(root.join("nested/data.txt"), "safe\n").expect("写入夹具文件");
    fs::write(root.join(".gitattributes"), "*.txt text eol=crlf\n").expect("写入夹具文件");
    git_fixture(git, root, &["add", "--all"]).await;
    git_fixture(
        git,
        root,
        &[
            "-c",
            "user.name=Soxu",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
    )
    .await;
    git_fixture(git, root, &["rev-parse", "HEAD"]).await
}

async fn git_fixture(probe: &GitProbe, root: &Path, args: &[&str]) -> String {
    git(probe, root, args).await
}

#[tokio::test]
async fn sterile_checkout_ignores_hooks_and_uses_only_allowed_attributes() {
    let fixture = Fixture::new();
    let probe = GitProbe::discover().expect("发现本机 Git");
    let source = fixture.path().join("source");
    let oid = seed(&probe, &source).await;
    let hooks = source.join(".git/hooks-sentinel");
    fs::create_dir(&hooks).expect("创建夹具路径");
    let hook = hooks.join("post-checkout");
    fs::write(&hook, "#!/bin/sh\nprintf ran > hook-ran\nexit 88\n").expect("写入夹具文件");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).expect("核对夹具操作结果");
    }
    git_fixture(
        &probe,
        &source,
        &[
            "config",
            "core.hooksPath",
            hooks.to_str().expect("核对夹具操作结果"),
        ],
    )
    .await;
    git_fixture(&probe, &source, &["config", "submodule.recurse", "true"]).await;
    git_fixture(
        &probe,
        &source,
        &[
            "config",
            "remote.origin.url",
            "https://invalid.invalid/sentinel",
        ],
    )
    .await;
    let sandbox = Sandbox::create(&fixture.path().join("stage"), &oid).expect("创建夹具路径");
    sandbox
        .snapshot(&source.join(".git"), &source.join(".git"))
        .expect("核对夹具操作结果");
    let (_guard, mut cancel) = oneshot::channel();
    let tree = sandbox
        .preflight(&probe, &oid, &mut cancel)
        .await
        .expect("校验夹具提交");
    assert_eq!(tree.files.get("nested/data.txt"), Some(&false));
    assert!(tree.directories.contains("nested"));
    sandbox
        .materialize(&probe, &oid, &tree, &mut cancel)
        .await
        .expect("核对夹具操作结果");
    assert_eq!(
        fs::read(sandbox.work.join("nested/data.txt")).expect("读取夹具文件"),
        b"safe\r\n"
    );
    assert!(!sandbox.work.join("hook-ran").exists());
    assert!(!source.join("hook-ran").exists());
}

#[tokio::test]
async fn preflight_rejects_unsafe_config_and_missing_blob_before_index_creation() {
    let fixture = Fixture::new();
    let probe = GitProbe::discover().expect("发现本机 Git");
    let source = fixture.path().join("source");
    let oid = seed(&probe, &source).await;
    let git_dir = source.join(".git");
    let (_guard, mut cancel) = oneshot::channel();
    for (key, value) in [
        ("filter.fixture.smudge", "exit 91"),
        ("filter.fixture.process", "exit 92"),
        ("include.path", "outside"),
        ("remote.origin.promisor", "true"),
        ("core.sparseCheckout", "true"),
        ("extensions.worktreeConfig", "true"),
    ] {
        git_fixture(
            &probe,
            &source,
            &["config", "--file", ".git/config", key, value],
        )
        .await;
        let stage = fixture.path().join(key.replace('.', "-"));
        let sandbox = Sandbox::create(&stage, &oid).expect("创建夹具路径");
        sandbox
            .snapshot(&git_dir, &git_dir)
            .expect("核对夹具操作结果");
        assert!(
            sandbox.preflight(&probe, &oid, &mut cancel).await.is_err(),
            "{key}"
        );
        assert!(!sandbox.admin.join("index").exists());
        git_fixture(
            &probe,
            &source,
            &["config", "--file", ".git/config", "--unset", key],
        )
        .await;
    }
    let blob = git_fixture(&probe, &source, &["rev-parse", "HEAD:nested/data.txt"]).await;
    let blob = git_dir.join("objects").join(&blob[..2]).join(&blob[2..]);
    fs::rename(blob, fixture.path().join("missing-blob")).expect("读取已核实的夹具记录");
    let sandbox = Sandbox::create(&fixture.path().join("missing"), &oid).expect("创建夹具路径");
    sandbox
        .snapshot(&git_dir, &git_dir)
        .expect("核对夹具操作结果");
    assert!(sandbox.preflight(&probe, &oid, &mut cancel).await.is_err());
    assert!(!sandbox.admin.join("index").exists());
}

#[test]
fn validates_paths_modes_and_attributes_before_materialization() {
    let oid = "1234567890123456789012345678901234567890";
    for tree in [
        format!("160000 commit {oid} 6\tmodule\0"),
        format!("120000 blob {oid} 6\tlink\0"),
        format!("100644 blob {oid} 6\tFoo/a\0"),
        format!(
            "100644 blob {oid} 6\tFoo/a\0{second}",
            second = format_args!("100644 blob {oid} 6\tfoo/b\0")
        ),
        format!("100644 blob {oid} 6\tx/.git/evil\0"),
        format!("100644 blob {oid} 6\tx/../escape\0"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            parse_tree(tree.1.as_bytes(), 40).is_ok(),
            tree.0 == 2,
            "{}",
            tree.1
        );
    }
    for attributes in [
        b"* filter=external\n".as_slice(),
        b"[attr]private filter=x",
        b"* working-tree-encoding=UTF-16",
    ] {
        assert!(check_attributes(attributes).is_err());
    }
}

#[test]
fn timeout_fixture_child() {
    if std::env::var_os("SOXU_CHECKOUT_TEST_CHILD").is_some() {
        use std::io::Write;
        assert_eq!(io::stdin().read(&mut [0]).expect("枚举夹具目录"), 0);
        io::stdout().write_all(b"ready").expect("核对夹具操作结果");
        io::stdout().flush().expect("核对夹具操作结果");
        std::thread::sleep(Duration::from_secs(10));
    }
}

#[tokio::test]
async fn timeout_and_cancellation_both_stop_and_reap_the_native_child() {
    fn command() -> Command {
        let mut command = Command::new(std::env::current_exe().expect("核对夹具操作结果"));
        command.env("SOXU_CHECKOUT_TEST_CHILD", "1").args([
            "--exact",
            "worktree_checkout::sandbox::tests::timeout_fixture_child",
            "--nocapture",
        ]);
        command
    }
    let (_guard, mut cancel) = oneshot::channel();
    assert!(matches!(
        run_child(&mut command(), &mut cancel, Duration::from_millis(150)).await,
        Err(CheckoutError::Git(GitError::Timeout))
    ));
    let (guard, mut cancel) = oneshot::channel();
    let mut child = command();
    let task =
        tokio::spawn(
            async move { run_child(&mut child, &mut cancel, Duration::from_secs(5)).await },
        );
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(guard);
    assert!(matches!(
        task.await.expect("等待夹具任务退出"),
        Err(CheckoutError::Cancelled)
    ));
}
