use std::collections::hash_map::RandomState;
use std::ffi::OsString;
use std::hash::BuildHasher;
use std::io::{Read, Seek};
use std::path::Path;
use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::CheckpointError;
use crate::GitProbe;

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) fn id() -> String {
    let seed = (
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    );
    format!(
        "{:016x}{:016x}",
        RandomState::new().hash_one(seed),
        RandomState::new().hash_one(seed)
    )
}

#[derive(Clone, Copy)]
pub(super) struct Git<'a> {
    pub(super) probe: &'a GitProbe,
    pub(super) root: &'a Path,
    pub(super) index: &'a Path,
    pub(super) hooks: &'a Path,
    pub(super) deadline: Instant,
    pub(super) max_output: u64,
    pub(super) timeout: Duration,
}

impl Git<'_> {
    pub(super) async fn run(
        &self,
        args: &[OsString],
        input: Option<&[u8]>,
    ) -> Result<Vec<u8>, CheckpointError> {
        let output = self.output(args, input, None).await?;
        success(output)
    }

    pub(super) async fn text(&self, args: &[&str]) -> Result<Vec<u8>, CheckpointError> {
        self.run(&args.iter().map(OsString::from).collect::<Vec<_>>(), None)
            .await
    }

    pub(super) async fn accepted(
        &self,
        args: &[OsString],
        input: Option<&[u8]>,
        codes: &[i32],
    ) -> Result<Vec<u8>, CheckpointError> {
        let output = self.output(args, input, None).await?;
        if output.status.code().is_some_and(|c| codes.contains(&c)) {
            Ok(output.stdout)
        } else {
            success(output)
        }
    }

    pub(super) async fn hash_file(
        &self,
        file: &mut std::fs::File,
        approved: &[u8],
    ) -> Result<String, CheckpointError> {
        file.rewind()?;
        let args = strings(&["hash-object", "-w", "--stdin"]);
        oid(&success(
            self.output(&args, None, Some((file, approved))).await?,
        )?)
    }

    pub(super) async fn digest(&self, bytes: &[u8]) -> Result<String, CheckpointError> {
        oid(&self
            .run(&strings(&["hash-object", "--stdin"]), Some(bytes))
            .await?)
    }

    async fn output(
        &self,
        args: &[OsString],
        input: Option<&[u8]>,
        mut file: Option<(&mut std::fs::File, &[u8])>,
    ) -> Result<Output, CheckpointError> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(CheckpointError::Timeout)?;
        #[cfg(windows)]
        let index = crate::worktree_lifecycle::native_command_path(self.index)
            .map_err(|_| CheckpointError::Invalid("私有 index 路径无法无损传给 Git"))?;
        #[cfg(not(windows))]
        let index = self.index;
        #[cfg(windows)]
        let hooks = crate::worktree_lifecycle::native_command_path(self.hooks)
            .map_err(|_| CheckpointError::Invalid("hooks 路径无法无损传给 Git"))?;
        #[cfg(not(windows))]
        let hooks = self.hooks;
        let mut command = self.probe.command(self.root);
        command
            .env("GIT_INDEX_FILE", index)
            .env("GIT_AUTHOR_NAME", "Soxu Checkpoint")
            .env("GIT_AUTHOR_EMAIL", "checkpoint@soxu.invalid")
            .env("GIT_COMMITTER_NAME", "Soxu Checkpoint")
            .env("GIT_COMMITTER_EMAIL", "checkpoint@soxu.invalid")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", crate::probe::null_device())
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_LFS_SKIP_SMUDGE", "1")
            .env("GIT_PAGER", "cat")
            .env("GIT_EXTERNAL_DIFF", "")
            .args([
                "-c",
                "core.logAllRefUpdates=false",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.untrackedCache=false",
                "-c",
                "gc.auto=0",
                "-c",
                "maintenance.auto=false",
                "-c",
                "core.splitIndex=false",
                "-c",
                "core.sparseCheckout=false",
                "-c",
                "core.attributesFile=",
                "-c",
                "core.excludesFile=",
                "-c",
                "diff.external=",
            ])
            .args([
                "--no-replace-objects",
                "-c",
                "core.fsync=committed",
                "-c",
                "core.fsyncMethod=fsync",
            ])
            .arg("-c")
            .arg({
                let mut setting = OsString::from("core.hooksPath=");
                setting.push(hooks.as_os_str());
                setting
            })
            .args(args)
            .stdin(Stdio::piped());
        let mut child = command.spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or(CheckpointError::Invalid("缺少 stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(CheckpointError::Invalid("缺少 stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(CheckpointError::Invalid("缺少 stderr"))?;
        let result = tokio::time::timeout(remaining.min(self.timeout), async {
            let write = async {
                if let Some(bytes) = input {
                    stdin.write_all(bytes).await?;
                }
                if let Some((file, mut approved)) = file.take() {
                    let mut buffer = vec![0; 65536];
                    let mut reader = file.take(approved.len() as u64 + 1);
                    loop {
                        let n = reader.read(&mut buffer)?;
                        if n == 0 {
                            break;
                        }
                        // 审核后若有并发写入，只允许已通过隐私检查的前缀进入对象库。
                        if !approved.starts_with(&buffer[..n]) {
                            return Err(CheckpointError::Drift);
                        }
                        stdin.write_all(&buffer[..n]).await?;
                        approved = &approved[n..];
                    }
                    if !approved.is_empty() {
                        return Err(CheckpointError::Drift);
                    }
                }
                stdin.shutdown().await?;
                drop(stdin);
                Ok::<(), CheckpointError>(())
            };
            let (_, stdout, stderr, status) = tokio::try_join!(
                write,
                bounded(stdout, self.max_output),
                bounded(stderr, self.max_output.min(65536)),
                async { child.wait().await.map_err(CheckpointError::from) }
            )?;
            Ok::<_, CheckpointError>(Output {
                status,
                stdout,
                stderr,
            })
        })
        .await
        .unwrap_or(Err(CheckpointError::Timeout));
        if result.is_err() {
            // 超时 Future 已先释放 stdin/stdout/stderr，再终止并回收直接子进程。
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        result
    }
}

async fn bounded(reader: impl AsyncRead + Unpin, limit: u64) -> Result<Vec<u8>, CheckpointError> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes).await?;
    if bytes.len() as u64 > limit {
        return Err(CheckpointError::Budget("Git 输出超限"));
    }
    Ok(bytes)
}

fn success(output: Output) -> Result<Vec<u8>, CheckpointError> {
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(CheckpointError::Git {
            code: output.status.code(),
            stderr: output.stderr,
        })
    }
}

pub(super) fn strings(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

pub(super) fn oid(bytes: &[u8]) -> Result<String, CheckpointError> {
    let value = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if !matches!(value.len(), 40 | 64) || !value.iter().all(u8::is_ascii_hexdigit) {
        return Err(CheckpointError::Invalid("无效的完整 OID"));
    }
    String::from_utf8(value.to_vec()).map_err(|_| CheckpointError::Invalid("OID 编码"))
}
