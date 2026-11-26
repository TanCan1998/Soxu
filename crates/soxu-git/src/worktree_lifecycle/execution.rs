use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::ExecutionResult;

pub(super) async fn run(command: &mut Command, limit: u64, timeout: Duration) -> ExecutionResult {
    let Ok(mut child) = command.spawn() else {
        return ExecutionResult::SpawnFailed;
    };
    let result = {
        // 读取 future 独占管道。超时或读取失败时，离开此作用域先释放管道，再停止子进程。
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child = &mut child;
        tokio::time::timeout(timeout, async move {
            let stdout = stdout.ok_or("missing_stdout")?;
            let stderr = stderr.ok_or("missing_stderr")?;
            let (_, _, status) = tokio::try_join!(
                count_output(stdout, limit),
                count_output(stderr, limit),
                async { child.wait().await.map_err(|_| "wait_io") },
            )?;
            Ok(status)
        })
        .await
    };
    let reason = match result {
        Ok(Ok(status)) if status.success() => return ExecutionResult::Succeeded,
        Ok(Ok(status)) => {
            return ExecutionResult::Failed {
                exit_code: status.code(),
            };
        }
        Ok(Err(reason)) => reason,
        Err(_) => "timeout",
    };
    let stop_confirmed = match child.try_wait() {
        Ok(Some(_)) => true,
        _ => {
            let _ = child.start_kill();
            let reap_timeout = timeout.max(Duration::from_secs(2));
            matches!(
                tokio::time::timeout(reap_timeout, child.wait()).await,
                Ok(Ok(_))
            )
        }
    };
    ExecutionResult::Unknown {
        stop_confirmed,
        reason: reason.to_owned(),
    }
}

async fn count_output(reader: impl AsyncRead + Unpin, limit: u64) -> Result<(), &'static str> {
    let bytes = tokio::io::copy(
        &mut reader.take(limit.saturating_add(1)),
        &mut tokio::io::sink(),
    )
    .await
    .map_err(|_| "output_io")?;
    if bytes > limit {
        return Err("output_limit");
    }
    Ok(())
}

#[cfg(test)]
mod native_process_tests {
    use std::process::Stdio;

    use super::*;

    // 直接复用当前原生测试二进制，不依赖 shell 或目录身份后端。
    #[test]
    fn execution_child_fixture() {
        let Ok(mode) = std::env::var("SOXU_WORKTREE_EXECUTION_FIXTURE") else {
            return;
        };
        match mode.as_str() {
            "failed" => std::process::exit(37),
            "output" => {
                use std::io::Write;
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(&[b'x'; 4096]).expect("输出夹具字节");
                stdout.flush().expect("刷新夹具输出");
            }
            "timeout" => std::thread::sleep(Duration::from_secs(5)),
            "success" => {}
            _ => panic!("未知执行夹具模式"),
        }
    }

    #[tokio::test]
    async fn native_execution_separates_spawn_exit_limits_and_confirmed_stops() {
        let executable = std::env::current_exe().expect("取得原生测试入口");
        // 可执行文件不可能同时是目录，因此该子路径确定不能启动。
        let mut missing = Command::new(executable.join("missing-program"));
        assert_eq!(
            run(&mut missing, 1024, Duration::from_secs(5)).await,
            ExecutionResult::SpawnFailed
        );
        for (mode, limit, deadline, expected) in [
            (
                "success",
                8192,
                Duration::from_secs(5),
                ExecutionResult::Succeeded,
            ),
            (
                "failed",
                8192,
                Duration::from_secs(5),
                ExecutionResult::Failed {
                    exit_code: Some(37),
                },
            ),
            (
                "output",
                1024,
                Duration::from_secs(5),
                ExecutionResult::Unknown {
                    stop_confirmed: true,
                    reason: "output_limit".to_owned(),
                },
            ),
            (
                "timeout",
                8192,
                Duration::from_millis(400),
                ExecutionResult::Unknown {
                    stop_confirmed: true,
                    reason: "timeout".to_owned(),
                },
            ),
        ] {
            let mut command = Command::new(std::env::current_exe().expect("取得原生测试入口"));
            command
                .args([
                    "--exact",
                    "worktree_lifecycle::execution::native_process_tests::execution_child_fixture",
                    "--nocapture",
                ])
                .env("SOXU_WORKTREE_EXECUTION_FIXTURE", mode)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            assert_eq!(
                run(&mut command, limit, deadline).await,
                expected,
                "执行模式 {mode}"
            );
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::process::Stdio;

    use super::*;

    #[tokio::test]
    async fn separates_spawn_exit_and_confirmed_stops() {
        let mut missing = Command::new("/dev/null/soxu-worktree-test");
        let timeout = Duration::from_secs(2);
        assert_eq!(
            run(&mut missing, 64, timeout).await,
            ExecutionResult::SpawnFailed
        );
        for (script, deadline, expected) in [
            (
                "exit 37",
                timeout,
                ExecutionResult::Failed {
                    exit_code: Some(37),
                },
            ),
            (
                "printf 'more than eight bytes'",
                timeout,
                ExecutionResult::Unknown {
                    stop_confirmed: true,
                    reason: "output_limit".to_owned(),
                },
            ),
            (
                "exec sleep 10",
                Duration::from_millis(100),
                ExecutionResult::Unknown {
                    stop_confirmed: true,
                    reason: "timeout".to_owned(),
                },
            ),
        ] {
            let mut command = Command::new("/bin/sh");
            command
                .args(["-c", script])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            assert_eq!(run(&mut command, 8, deadline).await, expected);
        }
    }
}
