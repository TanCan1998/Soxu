use std::io::Write;

use super::*;

#[test]
fn subprocess_fixture() {
    let Ok(mode) = std::env::var("SOXU_GIT_TEST_OUTPUT_MODE") else {
        return;
    };
    let bytes = vec![b'x'; 8192];
    match mode.as_str() {
        "stdout" => {
            std::io::stdout().write_all(&bytes).expect("夹具 stdout");
            std::io::stdout().flush().expect("刷新 stdout");
        }
        "stderr" => {
            std::io::stderr().write_all(&bytes).expect("夹具 stderr");
            std::io::stderr().flush().expect("刷新 stderr");
        }
        "timeout" => {}
        _ => panic!("未知夹具模式"),
    }
    std::thread::sleep(Duration::from_secs(10));
}

fn subprocess(mode: &str) -> Child {
    Command::new(std::env::current_exe().expect("测试原生入口"))
        .args(["--exact", "probe::tests::subprocess_fixture", "--nocapture"])
        .env("SOXU_GIT_TEST_OUTPUT_MODE", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("启动临时子进程")
}

#[tokio::test]
async fn output_overflow_wins_over_timeout_and_reaps_both_streams() {
    for mode in ["stdout", "stderr"] {
        let mut child = subprocess(mode);
        let result = collect_output(&mut child, 4096, Duration::from_secs(3)).await;
        assert!(
            matches!(result, Err(GitError::OutputLimit)),
            "{mode}: {result:?}"
        );
        assert!(child.try_wait().expect("回收后状态").is_some());
        assert!(child.id().is_none(), "超限进程必须已等待回收");
    }
}

#[tokio::test]
async fn timeout_awaits_direct_child_reaping() {
    let mut child = subprocess("timeout");
    let result = collect_output(&mut child, 4096, Duration::from_millis(200)).await;
    assert!(matches!(result, Err(GitError::Timeout)));
    assert!(child.try_wait().expect("回收后状态").is_some());
    assert!(child.id().is_none(), "超时进程必须已等待回收");
}

#[tokio::test]
async fn exact_output_limit_is_not_overflow() {
    assert_eq!(
        read_bounded(b"1234".as_slice(), 4).await.expect("精确上限"),
        b"1234"
    );
    assert!(matches!(
        read_bounded(b"12345".as_slice(), 4).await,
        Err(GitError::OutputLimit)
    ));
}
