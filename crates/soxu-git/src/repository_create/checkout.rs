//! 受控检出发布：只读枚举提交树、完整预检对象与属性，再按固定规则落盘并建立匹配 HEAD 的 index。
//!
//! 这里从不调用 `git checkout` / `git checkout-index`，因此不会执行钩子、LFS、子模块、外部过滤器或项目脚本。
//! 预检（树条目、对象类型与体积、属性内容）全部完成后才开始写入；任一步失败都不创建任何工作树文件。

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

/// 预检并发布指定提交的工作树与 index，返回已发布条目数。
pub(super) async fn publish_tree(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    oid: &str,
    entries: &[TreeEntry],
) -> Result<usize, RepositoryCreateError> {
    precheck_attributes(sandbox, program, target, entries).await?;
    publish_files(sandbox, program, target, entries).await?;
    write_index(sandbox, program, target, oid, entries).await?;
    verify_worktree_matches_head(sandbox, program, target, oid).await?;
    Ok(entries.len())
}

/// 只读枚举并预检提交树，不做任何发布。
pub(super) async fn precheck_tree(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    oid: &str,
) -> Result<Vec<TreeEntry>, RepositoryCreateError> {
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
    precheck_objects(sandbox, program, target, &entries).await?;
    precheck_attributes(sandbox, program, target, &entries).await?;
    Ok(entries)
}

#[derive(Debug, Clone)]
pub(super) struct TreeEntry {
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

/// 用 `cat-file --batch-check` 一次性预检全部对象的存在性、类型与体积。
async fn precheck_objects(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    entries: &[TreeEntry],
) -> Result<(), RepositoryCreateError> {
    if entries.is_empty() {
        return Ok(());
    }
    let request = sandbox.root().join("cat-file-check.txt");
    let mut batch = File::create(&request).map_err(RepositoryCreateError::Io)?;
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
        .args(["cat-file", "--batch-check"])
        .stdin(Stdio::from(
            File::open(&request).map_err(RepositoryCreateError::Io)?,
        ));
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() {
        return Err(RepositoryCreateError::UnsafeContent("对象预检失败"));
    }
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|_| RepositoryCreateError::UnsafeContent("对象预检输出不是合法 UTF-8"))?;
    let mut total = 0_u64;
    let mut seen = 0_usize;
    for (line, entry) in text.lines().zip(entries.iter()) {
        let mut parts = line.split(' ');
        let oid = parts.next().unwrap_or_default();
        let kind = parts.next().unwrap_or_default();
        let size = parts.next().unwrap_or_default();
        if oid != entry.oid || kind != "blob" || parts.next().is_some() {
            return Err(RepositoryCreateError::UnsafeContent("对象预检类型不符"));
        }
        let size: u64 = size
            .parse()
            .map_err(|_| RepositoryCreateError::UnsafeContent("对象预检体积无效"))?;
        if size > MAX_FILE_BYTES || total.saturating_add(size) > MAX_TOTAL_BYTES {
            return Err(RepositoryCreateError::UnsafeContent("对象体积超过发布上限"));
        }
        total += size;
        seen += 1;
    }
    if seen != entries.len() {
        return Err(RepositoryCreateError::UnsafeContent("对象预检条目数不符"));
    }
    Ok(())
}

/// 发布前读取全部 `.gitattributes` 对象内容并校验，避免"先写再拒"。
async fn precheck_attributes(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    entries: &[TreeEntry],
) -> Result<(), RepositoryCreateError> {
    for entry in entries
        .iter()
        .filter(|entry| entry.path.ends_with(".gitattributes"))
    {
        let mut command = sandbox.command(program, sandbox.root());
        command
            .arg("-C")
            .arg(target)
            .args(["cat-file", "blob", &entry.oid]);
        let output =
            sandbox::run(&mut command, MAX_FILE_BYTES + 1, sandbox::COMMAND_TIMEOUT).await?;
        if !output.status.success() {
            return Err(RepositoryCreateError::UnsafeContent("无法读取属性对象"));
        }
        super::config_scan::reject_filter_attributes(&output.stdout)?;
    }
    Ok(())
}

async fn publish_files(
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

/// 建立与指定提交完全一致的 `.git/index`，并与提交树逐条比对。
async fn write_index(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    oid: &str,
    entries: &[TreeEntry],
) -> Result<(), RepositoryCreateError> {
    let mut command = sandbox.command(program, sandbox.root());
    command.arg("-C").arg(target).args(["read-tree", oid]);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() {
        return Err(RepositoryCreateError::UnsafeContent(
            "无法建立与提交匹配的 index",
        ));
    }
    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(target)
        .args(["ls-files", "--stage", "-z"]);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() || output.stdout != expected_index(entries) {
        return Err(RepositoryCreateError::UnsafeContent("index 与提交树不一致"));
    }
    Ok(())
}

fn expected_index(entries: &[TreeEntry]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in entries {
        let line = format!("{} {} 0\t{}\0", entry.mode, entry.oid, entry.path);
        bytes.extend_from_slice(line.as_bytes());
    }
    bytes
}

/// 发布后复核：HEAD、index 与工作树三者一致，`git status` 必须为空。
async fn verify_worktree_matches_head(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    oid: &str,
) -> Result<(), RepositoryCreateError> {
    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(target)
        .args(["diff-index", "--cached", "--quiet", oid, "--"]);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() {
        return Err(RepositoryCreateError::UnsafeContent("index 与 HEAD 不一致"));
    }
    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(target)
        .args(["status", "--porcelain=v2", "-z", "--untracked-files=all"]);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() || !output.stdout.is_empty() {
        return Err(RepositoryCreateError::UnsafeContent(
            "克隆工作树与 HEAD 不一致",
        ));
    }
    Ok(())
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

/// 目录 HEAD 的只读判定：区分确证提交、确证 unborn 与无法核验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HeadState {
    Commit(String),
    Unborn(String),
    Unverifiable(&'static str),
}

/// 只读解析指定目录的 HEAD；三分结果不允许被当作同一种"没有提交"。
pub(super) async fn probe_head(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
) -> Result<HeadState, RepositoryCreateError> {
    if let Some(bytes) = capture(
        sandbox,
        program,
        target,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )
    .await?
    {
        let oid = String::from_utf8_lossy(&bytes).trim().to_owned();
        let sized = oid.len() == 40 || oid.len() == 64;
        if !sized || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(HeadState::Unverifiable("HEAD 不是完整提交 OID"));
        }
        return Ok(HeadState::Commit(oid));
    }
    // 无提交：先证 HEAD 是分支引用，再证该分支确实不存在，才能声称 unborn。
    let Some(bytes) = capture(sandbox, program, target, &["symbolic-ref", "-q", "HEAD"]).await?
    else {
        return Ok(HeadState::Unverifiable("HEAD 既不是提交也不是分支引用"));
    };
    let branch = String::from_utf8_lossy(&bytes).trim().to_owned();
    if !branch.starts_with("refs/heads/") {
        return Ok(HeadState::Unverifiable("HEAD 指向非分支引用"));
    }
    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(target)
        .args(["show-ref", "--exists", &branch]);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    // 只有 Git 官方约定的 exit 2 才表示"引用不存在"；其余非零一律视为无法核验。
    match output.status.code() {
        Some(0) => Ok(HeadState::Unverifiable("分支存在但提交对象不可解析")),
        Some(2) => Ok(HeadState::Unborn(branch)),
        _ => Ok(HeadState::Unverifiable("无法确认分支引用是否存在")),
    }
}

async fn capture(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    arguments: &[&str],
) -> Result<Option<Vec<u8>>, RepositoryCreateError> {
    let mut command = sandbox.command(program, sandbox.root());
    command.arg("-C").arg(target).args(arguments);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    Ok(output.status.success().then_some(output.stdout))
}

/// 发布后在最终目标复核 HEAD、index 与工作树内容一致。
///
/// `expected` 为捕获时的固定 HEAD：有值必须解出且相等，无值必须是确证 unborn。
/// 任何无法解释的失败都不授予 `Verified`。
pub(super) async fn verify_published(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    expected: Option<&str>,
) -> Result<(), RepositoryCreateError> {
    let oid = match probe_head(sandbox, program, target).await? {
        HeadState::Unverifiable(reason) => {
            return Err(RepositoryCreateError::UnsafeContent(reason));
        }
        HeadState::Unborn(_) => {
            if expected.is_some() {
                return Err(RepositoryCreateError::UnsafeContent(
                    "预期固定 HEAD 但目标是 unborn 引用",
                ));
            }
            let mut command = sandbox.command(program, sandbox.root());
            command
                .arg("-C")
                .arg(target)
                .args(["ls-files", "--stage", "-z"]);
            let output = sandbox::run(
                &mut command,
                sandbox::OUTPUT_LIMIT,
                sandbox::COMMAND_TIMEOUT,
            )
            .await?;
            if !output.status.success() || !output.stdout.is_empty() {
                return Err(RepositoryCreateError::UnsafeContent(
                    "空仓库的 index 不应包含条目",
                ));
            }
            return Ok(());
        }
        HeadState::Commit(oid) => {
            // 固定源为 unborn 时，目标不得变成提交态。
            let expected = expected.ok_or(RepositoryCreateError::UnsafeContent(
                "固定源为 unborn，但目标出现了提交",
            ))?;
            if oid != expected {
                return Err(RepositoryCreateError::UnsafeContent(
                    "发布后的 HEAD 与捕获时的固定 HEAD 不一致",
                ));
            }
            oid
        }
    };
    let entries = precheck_tree(sandbox, program, target, &oid).await?;
    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(target)
        .args(["ls-files", "--stage", "-z"]);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() || output.stdout != expected_index(&entries) {
        return Err(RepositoryCreateError::UnsafeContent(
            "发布后的 index 与 HEAD 不一致",
        ));
    }
    verify_worktree_matches_head(sandbox, program, target, &oid).await
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
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unverifiable_head_is_not_accepted_as_published() {
        let root = std::env::temp_dir().join(format!("soxu-rc-head-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("临时根");
        // 生产路径校验拒绝含链接的祖先，Unix 上夹具根须取自规范化后的真实路径。
        #[cfg(unix)]
        let root = root.canonicalize().expect("规范化临时根");
        let state = root.join("state");
        let target = root.join("target");
        std::fs::create_dir_all(&state).expect("state");
        std::fs::create_dir_all(target.join(".git").join("refs").join("heads")).expect("git");
        std::fs::write(target.join(".git").join("HEAD"), b"ref: refs/heads/main\n").expect("HEAD");
        std::fs::write(
            target.join(".git").join("refs").join("heads").join("main"),
            b"0000000000000000000000000000000000000000\n",
        )
        .expect("ref");
        let sandbox = Sandbox::prepare(&state, "head-1").expect("sandbox");
        let program = crate::GitProbe::discover()
            .expect("本机 Git")
            .program()
            .to_owned();
        let observed = probe_head(&sandbox, &program, &target)
            .await
            .expect("probe");
        assert!(
            matches!(observed, HeadState::Unverifiable(_)),
            "分支存在但对象不可解析必须判为无法核验：{observed:?}"
        );
        let error = verify_published(
            &sandbox,
            &program,
            &target,
            Some("0000000000000000000000000000000000000000"),
        )
        .await
        .expect_err("无法核验的 HEAD 不得通过发布复核");
        assert!(matches!(error, RepositoryCreateError::UnsafeContent(_)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn fixed_source_unborn_but_target_commit_is_rejected() {
        let root = std::env::temp_dir().join(format!("soxu-rc-unborn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("临时根");
        // 生产路径校验拒绝含链接的祖先，Unix 上夹具根须取自规范化后的真实路径。
        #[cfg(unix)]
        let root = root.canonicalize().expect("规范化临时根");
        let state = root.join("state");
        let target = root.join("target");
        std::fs::create_dir_all(&state).expect("state");
        std::fs::create_dir_all(&target).expect("target");
        let sandbox = Sandbox::prepare(&state, "unborn-1").expect("sandbox");
        let program = crate::GitProbe::discover()
            .expect("本机 Git")
            .program()
            .to_owned();
        let mut command = sandbox.command(&program, sandbox.root());
        command
            .arg("init")
            .args(["--template=", "--initial-branch=main", "--"])
            .arg(&target);
        assert!(
            sandbox::run(
                &mut command,
                sandbox::OUTPUT_LIMIT,
                sandbox::COMMAND_TIMEOUT
            )
            .await
            .expect("init")
            .status
            .success()
        );
        let mut command = sandbox.command(&program, &target);
        command.args([
            "-c",
            "user.name=Soxu Test",
            "-c",
            "user.email=soxu-test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "x",
        ]);
        assert!(
            sandbox::run(
                &mut command,
                sandbox::OUTPUT_LIMIT,
                sandbox::COMMAND_TIMEOUT
            )
            .await
            .expect("commit")
            .status
            .success()
        );
        let error = verify_published(&sandbox, &program, &target, None)
            .await
            .expect_err("固定源为 unborn 时目标不得变成提交态");
        assert!(matches!(error, RepositoryCreateError::UnsafeContent(_)));
        let _ = std::fs::remove_dir_all(&root);
    }
}
