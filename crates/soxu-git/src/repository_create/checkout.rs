//! 受控检出发布：只读枚举提交树、按固定规则预检，再用 `cat-file --batch` 逐对象落盘。
//!
//! 这里从不调用 `git checkout` / `git checkout-index`，因此不会执行钩子、LFS、子模块、外部过滤器或项目脚本。

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use super::RepositoryCreateError;
use super::platform;
use super::sandbox::{self, Sandbox};

/// 单次克隆允许发布的提交条目上限。
const MAX_ENTRIES: usize = 4096;
/// 单个发布文件体积上限。
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// 单次发布总体积上限。
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

/// 预检并发布指定提交的工作树，返回已发布条目数。
pub(super) async fn publish_tree(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    oid: &str,
) -> Result<usize, RepositoryCreateError> {
    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(target)
        .args(["ls-tree", "-r", "-z", "--full-tree", oid]);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() {
        return Err(RepositoryCreateError::UnsafeContent("无法枚举提交树"));
    }
    let entries = parse_tree(&output.stdout)?;
    validate_tree(&entries)?;
    publish(sandbox, program, target, &entries).await
}

#[derive(Debug, Clone)]
struct TreeEntry {
    mode: String,
    oid: String,
    path: String,
}

fn parse_tree(bytes: &[u8]) -> Result<Vec<TreeEntry>, RepositoryCreateError> {
    let mut entries = Vec::new();
    for record in bytes.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let split = record.iter().position(|byte| *byte == b'\t').ok_or(
            RepositoryCreateError::UnsafeContent("提交树条目缺少路径分隔符"),
        )?;
        let head = std::str::from_utf8(&record[..split])
            .map_err(|_| RepositoryCreateError::UnsafeContent("提交树条目不是合法 UTF-8"))?;
        let path = std::str::from_utf8(&record[split + 1..])
            .map_err(|_| RepositoryCreateError::UnsafeContent("提交树路径不是合法 UTF-8"))?;
        let mut parts = head.split(' ');
        let mode = parts.next().unwrap_or_default();
        let kind = parts.next().unwrap_or_default();
        let oid = parts.next().unwrap_or_default();
        if parts.next().is_some() || kind != "blob" {
            return Err(RepositoryCreateError::UnsafeContent("提交树条目类型"));
        }
        if entries.len() >= MAX_ENTRIES {
            return Err(RepositoryCreateError::UnsafeContent("提交条目超过上限"));
        }
        entries.push(TreeEntry {
            mode: mode.to_owned(),
            oid: oid.to_owned(),
            path: path.to_owned(),
        });
    }
    Ok(entries)
}

fn validate_tree(entries: &[TreeEntry]) -> Result<(), RepositoryCreateError> {
    let mut names: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for entry in entries {
        match entry.mode.as_str() {
            "100644" | "100755" => {}
            "120000" => return Err(RepositoryCreateError::UnsafeContent("提交含符号链接")),
            "160000" => {
                return Err(RepositoryCreateError::UnsafeContent("提交含子模块 gitlink"));
            }
            _ => return Err(RepositoryCreateError::UnsafeContent("提交含非普通文件模式")),
        }
        let sized = entry.oid.len() == 40 || entry.oid.len() == 64;
        if !sized || !entry.oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(RepositoryCreateError::UnsafeContent("提交条目对象 ID 无效"));
        }
        if !super::target::portable_relative(&entry.path) {
            return Err(RepositoryCreateError::UnsafeContent(
                "提交路径不是可移植相对路径",
            ));
        }
        let mut prefix = String::new();
        for component in entry.path.split('/') {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            remember(&mut names, &prefix, prefix.len() < entry.path.len())?;
        }
    }
    Ok(())
}

fn remember(
    names: &mut BTreeMap<String, (String, bool)>,
    path: &str,
    directory: bool,
) -> Result<(), RepositoryCreateError> {
    use std::collections::btree_map::Entry;
    match names.entry(path.to_ascii_lowercase()) {
        Entry::Occupied(entry) if directory && entry.get().1 && entry.get().0 == path => Ok(()),
        Entry::Occupied(_) => Err(RepositoryCreateError::UnsafeContent(
            "提交路径存在大小写别名或文件目录冲突",
        )),
        Entry::Vacant(entry) => {
            entry.insert((path.to_owned(), directory));
            Ok(())
        }
    }
}

async fn publish(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    entries: &[TreeEntry],
) -> Result<usize, RepositoryCreateError> {
    ensure_only_git_dir(target)?;
    for directory in directories_of(entries) {
        let path = target.join(&directory);
        match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata =
                    std::fs::symlink_metadata(&path).map_err(RepositoryCreateError::Io)?;
                if !platform::plain_directory(&metadata) {
                    return Err(RepositoryCreateError::UnsafeContent("发布目录冲突"));
                }
            }
            Err(error) => return Err(RepositoryCreateError::Io(error)),
        }
    }

    let batch_path = sandbox.root().join("cat-file-request.txt");
    let mut batch = File::create(&batch_path).map_err(RepositoryCreateError::Io)?;
    for entry in entries {
        batch
            .write_all(entry.oid.as_bytes())
            .and_then(|()| batch.write_all(b"\n"))
            .map_err(RepositoryCreateError::Io)?;
    }
    batch.sync_all().map_err(RepositoryCreateError::Io)?;
    drop(batch);

    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(target)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::from(
            File::open(&batch_path).map_err(RepositoryCreateError::Io)?,
        ));
    let mut child = command.spawn().map_err(RepositoryCreateError::Io)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(RepositoryCreateError::InvalidInput("缺少 stdout 管道"))?;
    let mut reader = BufReader::new(stdout);
    let stderr = child
        .stderr
        .take()
        .ok_or(RepositoryCreateError::InvalidInput("缺少 stderr 管道"))?;
    let drained = tokio::spawn(async move {
        let mut sink = Vec::new();
        let _ = stderr.take(64 * 1024).read_to_end(&mut sink).await;
    });

    let mut total = 0_u64;
    let mut written = 0_usize;
    let attempt = tokio::time::timeout(sandbox::COMMAND_TIMEOUT, async {
        for entry in entries {
            let contents = read_object(&mut reader, entry, &mut total).await?;
            if entry.path.ends_with(".gitattributes") {
                super::config_scan::reject_filter_attributes(&contents)?;
            }
            write_entry(target, entry, &contents)?;
            written += 1;
        }
        Ok::<(), RepositoryCreateError>(())
    })
    .await;
    match attempt {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            stop(&mut child, drained).await;
            return Err(error);
        }
        Err(_) => {
            stop(&mut child, drained).await;
            return Err(RepositoryCreateError::GitTimeout);
        }
    }
    let status = child.wait().await.map_err(RepositoryCreateError::Io)?;
    let _ = drained.await;
    if !status.success() {
        return Err(RepositoryCreateError::UnsafeContent("对象读取进程失败"));
    }
    Ok(written)
}

async fn stop(child: &mut tokio::process::Child, drained: tokio::task::JoinHandle<()>) {
    let _ = child.start_kill();
    let _ = child.wait().await;
    let _ = drained.await;
}

async fn read_object(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    entry: &TreeEntry,
    total: &mut u64,
) -> Result<Vec<u8>, RepositoryCreateError> {
    let mut header = Vec::new();
    reader
        .read_until(b'\n', &mut header)
        .await
        .map_err(RepositoryCreateError::Io)?;
    if !header.ends_with(b"\n") {
        return Err(RepositoryCreateError::UnsafeContent("对象读取被截断"));
    }
    let text = std::str::from_utf8(&header[..header.len() - 1])
        .map_err(|_| RepositoryCreateError::UnsafeContent("对象头不是合法 UTF-8"))?;
    let mut parts = text.split(' ');
    let oid = parts.next().unwrap_or_default();
    let kind = parts.next().unwrap_or_default();
    let size = parts.next().unwrap_or_default();
    if oid != entry.oid || kind != "blob" {
        return Err(RepositoryCreateError::UnsafeContent(
            "对象读取返回了非预期对象",
        ));
    }
    let size: u64 = size
        .parse()
        .map_err(|_| RepositoryCreateError::UnsafeContent("对象大小无效"))?;
    if size > MAX_FILE_BYTES || total.saturating_add(size) > MAX_TOTAL_BYTES {
        return Err(RepositoryCreateError::UnsafeContent("对象体积超过发布上限"));
    }
    *total += size;
    let mut contents = vec![
        0_u8;
        usize::try_from(size).map_err(|_| {
            RepositoryCreateError::UnsafeContent("对象体积无法表示")
        })?
    ];
    reader
        .read_exact(&mut contents)
        .await
        .map_err(RepositoryCreateError::Io)?;
    let mut trailer = [0_u8; 1];
    reader
        .read_exact(&mut trailer)
        .await
        .map_err(RepositoryCreateError::Io)?;
    if trailer[0] != b'\n' {
        return Err(RepositoryCreateError::UnsafeContent("对象内容分隔符无效"));
    }
    Ok(contents)
}

fn directories_of(entries: &[TreeEntry]) -> BTreeSet<String> {
    let mut directories = BTreeSet::new();
    for entry in entries {
        let components: Vec<&str> = entry.path.split('/').collect();
        let mut prefix = String::new();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            directories.insert(prefix.clone());
        }
    }
    directories
}

fn write_entry(
    target: &Path,
    entry: &TreeEntry,
    contents: &[u8],
) -> Result<(), RepositoryCreateError> {
    let path = target.join(&entry.path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(RepositoryCreateError::Io)?;
    let metadata = file.metadata().map_err(RepositoryCreateError::Io)?;
    if !metadata.is_file() || metadata.is_symlink() || platform::is_reparse(&metadata) {
        return Err(RepositoryCreateError::UnsafeContent("发布目标不是普通文件"));
    }
    file.write_all(contents)
        .map_err(RepositoryCreateError::Io)?;
    file.sync_all().map_err(RepositoryCreateError::Io)?;
    set_executable(&file, &entry.mode)?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(file: &File, mode: &str) -> Result<(), RepositoryCreateError> {
    use std::os::unix::fs::PermissionsExt;
    if mode != "100755" {
        return Ok(());
    }
    let mut permissions = file
        .metadata()
        .map_err(RepositoryCreateError::Io)?
        .permissions();
    permissions.set_mode(0o755);
    file.set_permissions(permissions)
        .map_err(RepositoryCreateError::Io)
}

#[cfg(not(unix))]
fn set_executable(_file: &File, _mode: &str) -> Result<(), RepositoryCreateError> {
    Ok(())
}

fn ensure_only_git_dir(target: &Path) -> Result<(), RepositoryCreateError> {
    let mut entries = std::fs::read_dir(target).map_err(RepositoryCreateError::Io)?;
    while let Some(entry) = entries
        .next()
        .transpose()
        .map_err(RepositoryCreateError::Io)?
    {
        if entry.file_name() != std::ffi::OsStr::new(".git") {
            return Err(RepositoryCreateError::UnsafeContent(
                "克隆产物目录含非预期内容",
            ));
        }
    }
    Ok(())
}
