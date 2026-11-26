use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::oneshot;

use super::CheckoutError;
use crate::probe::{COMMAND_TIMEOUT, OUTPUT_LIMIT};
use crate::worktree_lifecycle::{check_config, identity::is_reparse, native_command_path};
use crate::{GitError, GitProbe};

pub(super) const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CONFIG: u64 = 64 * 1024;

pub(super) struct Sandbox {
    pub(super) work: PathBuf,
    pub(super) admin: PathBuf,
}

pub(super) struct Tree {
    pub(super) files: BTreeMap<String, bool>,
    pub(super) directories: BTreeSet<String>,
    index: Vec<u8>,
    attributes: Vec<String>,
}

impl Sandbox {
    pub(super) fn create(root: &Path, oid: &str) -> Result<Self, CheckoutError> {
        let work = root.join("work");
        let admin = root.join("admin");
        let config: &[u8] = match oid.len() {
            40 => b"[core]\nrepositoryformatversion = 0\nbare = false\n",
            64 => b"[core]\nrepositoryformatversion = 1\nbare = false\n[extensions]\nobjectformat = sha256\n",
            _ => return Err(CheckoutError::Unsafe("提交 OID 格式")),
        };
        fs::create_dir(root)?;
        fs::create_dir(&work)?;
        fs::create_dir(&admin)?;
        fs::create_dir(admin.join("refs"))?;
        fs::create_dir(admin.join("objects"))?;
        fs::create_dir(admin.join("objects/pack"))?;
        fs::write(admin.join("HEAD"), format!("{oid}\n"))?;
        fs::write(admin.join("config"), config)?;
        Ok(Self { work, admin })
    }

    pub(super) fn snapshot(&self, common: &Path, admin: &Path) -> Result<(), CheckoutError> {
        fs::write(
            self.admin.join("repo-config"),
            repository_config(common, admin)?,
        )?;
        self.copy_objects(&common.join("objects"))
    }

    #[cfg(target_os = "linux")]
    pub(super) fn recheck_config(&self, common: &Path, admin: &Path) -> Result<(), CheckoutError> {
        if repository_config(common, admin)?
            != read_regular(&self.admin.join("repo-config"), MAX_CONFIG)?
        {
            return Err(CheckoutError::Unsafe("仓库配置在检出期间发生变化"));
        }
        Ok(())
    }

    fn copy_objects(&self, objects: &Path) -> Result<(), CheckoutError> {
        let pinned_objects = snapshot_directory(objects)?;
        let objects = pinned_objects.path();
        require_directory(&objects.join("info"))?;
        for name in ["alternates", "http-alternates", "promisor"] {
            require_absent(&objects.join("info").join(name), "外部对象来源")?;
        }
        let packs = objects.join("pack");
        require_directory(&packs)?;
        let destination = self.admin.join("objects");
        let mut budget = ObjectBudget {
            files: 100_000,
            bytes: MAX_BYTES,
        };
        for entry in fs::read_dir(objects)? {
            let prefix = entry?;
            let name = prefix.file_name();
            let Some(name) = name
                .to_str()
                .filter(|name| name.len() == 2 && hex(name.as_bytes()))
            else {
                continue;
            };
            let directory = snapshot_directory(&prefix.path())?;
            let base = destination.join(name);
            fs::create_dir(&base)?;
            for entry in fs::read_dir(directory.path())? {
                let entry = entry?;
                let name = entry.file_name();
                if !name
                    .to_str()
                    .is_some_and(|name| matches!(name.len(), 38 | 62) && hex(name.as_bytes()))
                {
                    return Err(CheckoutError::Unsafe("非标准 loose 对象路径"));
                }
                budget.copy(&entry.path(), &base.join(name))?;
            }
        }
        let packs = snapshot_directory(&packs)?;
        for entry in fs::read_dir(packs.path())? {
            let entry = entry?;
            let os_name = entry.file_name();
            let name = os_name
                .to_str()
                .ok_or(CheckoutError::Unsafe("pack 路径编码"))?;
            if matches!(name, "multi-pack-index" | "multi-pack-index.d") {
                continue;
            }
            let (oid, extension) = name
                .strip_prefix("pack-")
                .and_then(|name| name.split_once('.'))
                .ok_or(CheckoutError::Unsafe("非标准 pack 路径"))?;
            if !matches!(oid.len(), 40 | 64) || !hex(oid.as_bytes()) {
                return Err(CheckoutError::Unsafe("非标准 pack 路径"));
            }
            match extension {
                "pack" | "idx" => {
                    budget.copy(&entry.path(), &destination.join("pack").join(name))?
                }
                "bitmap" | "rev" | "mtimes" | "keep" => {}
                _ => return Err(CheckoutError::Unsafe("外部对象或未知 pack 来源")),
            }
        }
        Ok(())
    }

    pub(super) async fn preflight(
        &self,
        git: &GitProbe,
        oid: &str,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<Tree, CheckoutError> {
        let config = self
            .run(
                git,
                "config",
                &[
                    "--file",
                    "../admin/repo-config",
                    "--null",
                    "--list",
                    "--no-includes",
                ],
                cancel,
            )
            .await?;
        check_source_config(&config, oid.len())?;
        if self.run(git, "cat-file", &["-t", oid], cancel).await? != b"commit\n" {
            return Err(CheckoutError::Unsafe("固定 OID 不是提交"));
        }
        let files = self
            .run(
                git,
                "ls-tree",
                &["-r", "-l", "-z", "--full-tree", oid],
                cancel,
            )
            .await?;
        let tree = parse_tree(&files, oid.len())?;
        self.run(
            git,
            "fsck",
            &[
                "--full",
                "--no-reflogs",
                "--no-dangling",
                "--no-progress",
                oid,
            ],
            cancel,
        )
        .await?;
        for attribute_oid in &tree.attributes {
            let attributes = self
                .run(git, "cat-file", &["blob", attribute_oid], cancel)
                .await?;
            check_attributes(&attributes)?;
        }
        Ok(tree)
    }

    pub(super) async fn materialize(
        &self,
        git: &GitProbe,
        oid: &str,
        tree: &Tree,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<(), CheckoutError> {
        self.run(git, "read-tree", &[oid], cancel).await?;
        if self
            .run(git, "ls-files", &["--stage", "-z"], cancel)
            .await?
            != tree.index
        {
            return Err(CheckoutError::Unsafe("暂存 index 与提交树不一致"));
        }
        self.run(git, "checkout-index", &["--all"], cancel).await?;
        Ok(())
    }

    async fn run(
        &self,
        git: &GitProbe,
        operation: &'static str,
        args: &[&str],
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<Vec<u8>, CheckoutError> {
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let mut command = git.command(&self.work);
        command
            .env("GIT_DIR", native_command_path(&self.admin)?)
            .env("GIT_WORK_TREE", native_command_path(&self.work)?)
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_ALLOW_PROTOCOL", "")
            .env("GIT_PROTOCOL_FROM_USER", "0")
            .args([
                "-c",
                &format!("core.attributesFile={null}"),
                "-c",
                "core.protectNTFS=true",
                "-c",
                "core.protectHFS=true",
                "-c",
                "submodule.recurse=false",
            ])
            .arg(operation)
            .args(args);
        let output = run_child(&mut command, cancel, COMMAND_TIMEOUT).await?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(GitError::CommandFailed {
                operation,
                code: output.status.code(),
                stderr: output.stderr,
            }
            .into());
        }
        Ok(output.stdout)
    }
}

fn repository_config(common: &Path, admin: &Path) -> Result<Vec<u8>, CheckoutError> {
    for directory in [common.join("info"), admin.join("info")] {
        match directory.symlink_metadata() {
            Ok(metadata) if metadata.is_dir() && !is_reparse(&metadata) => {}
            Ok(_) => return Err(CheckoutError::Unsafe("外部属性目录")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        for name in ["attributes", "sparse-checkout", "grafts"] {
            require_absent(&directory.join(name), "外部属性、稀疏检出或 grafts")?;
        }
    }
    for path in [admin.join("config.worktree"), common.join("shallow")] {
        require_absent(&path, "工作树专属配置或浅克隆")?;
    }
    require_absent(&common.join("config.lock"), "仓库配置正在修改")?;
    read_regular(&common.join("config"), MAX_CONFIG)
}

struct ObjectBudget {
    files: usize,
    bytes: u64,
}

impl ObjectBudget {
    fn copy(&mut self, source: &Path, target: &Path) -> Result<(), CheckoutError> {
        self.files = self
            .files
            .checked_sub(1)
            .ok_or(CheckoutError::Unsafe("对象文件数量超限"))?;
        let input = open_regular(source)?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(target)?;
        let copied = io::copy(&mut input.take(self.bytes + 1), &mut output)?;
        self.bytes = self
            .bytes
            .checked_sub(copied)
            .ok_or(CheckoutError::Unsafe("对象总大小超限"))?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
use super::publication::PinnedDirectory;

#[cfg(target_os = "linux")]
fn snapshot_directory(path: &Path) -> Result<PinnedDirectory, CheckoutError> {
    PinnedDirectory::open(path)
}

#[cfg(not(target_os = "linux"))]
struct SnapshotDirectory(PathBuf);

#[cfg(not(target_os = "linux"))]
impl SnapshotDirectory {
    fn path(&self) -> &Path {
        &self.0
    }
}

#[cfg(not(target_os = "linux"))]
fn snapshot_directory(path: &Path) -> Result<SnapshotDirectory, CheckoutError> {
    require_directory(path)?;
    Ok(SnapshotDirectory(path.to_owned()))
}

pub(super) fn require_absent(path: &Path, reason: &'static str) -> Result<(), CheckoutError> {
    match path.symlink_metadata() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err(CheckoutError::Unsafe(reason)),
    }
}

fn require_directory(path: &Path) -> Result<(), CheckoutError> {
    let metadata = path.symlink_metadata()?;
    if !metadata.is_dir() || is_reparse(&metadata) {
        return Err(CheckoutError::Unsafe("对象目录类型"));
    }
    Ok(())
}

pub(super) fn open_regular(path: &Path) -> Result<File, CheckoutError> {
    let before = path.symlink_metadata()?;
    if !before.is_file() || is_reparse(&before) {
        return Err(CheckoutError::Unsafe("预期普通文件"));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Linux O_NOFOLLOW | O_NONBLOCK。
        options.custom_flags(0o400000 | 0o4000);
    }
    let file = options.open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || is_reparse(&opened) {
        return Err(CheckoutError::Unsafe("普通文件已替换"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (before.dev(), before.ino()) != (opened.dev(), opened.ino()) {
            return Err(CheckoutError::Unsafe("普通文件已替换"));
        }
    }
    Ok(file)
}

pub(super) fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, CheckoutError> {
    let mut bytes = Vec::new();
    open_regular(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(CheckoutError::Unsafe("文件读取上限"));
    }
    Ok(bytes)
}

fn check_source_config(config: &[u8], oid_size: usize) -> Result<(), CheckoutError> {
    for entry in config
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let mut parts = entry.splitn(2, |byte| *byte == b'\n');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        if key.starts_with(b"filter.")
            || matches!(
                key,
                b"core.attributesfile"
                    | b"core.sparsecheckout"
                    | b"core.sparsecheckoutcone"
                    | b"extensions.worktreeconfig"
                    | b"index.sparse"
                    | b"core.splitindex"
            )
        {
            return Err(CheckoutError::Unsafe("过滤器、属性、稀疏或工作树配置"));
        }
        if key == b"extensions.objectformat"
            && ((oid_size == 40 && !value.eq_ignore_ascii_case(b"sha1"))
                || (oid_size == 64 && !value.eq_ignore_ascii_case(b"sha256")))
        {
            return Err(CheckoutError::Unsafe("对象格式和固定 OID 不一致"));
        }
    }
    Ok(check_config(config)?)
}

fn check_attributes(bytes: &[u8]) -> Result<(), CheckoutError> {
    if bytes.len() as u64 > MAX_CONFIG
        || !bytes.is_ascii()
        || bytes
            .iter()
            .any(|byte| byte.is_ascii_control() && !matches!(byte, b'\n' | b'\r' | b'\t'))
    {
        return Err(CheckoutError::Unsafe("属性文件大小或编码"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| CheckoutError::Unsafe("属性编码"))?;
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let mut fields = line.split_ascii_whitespace();
        let pattern = fields.next().ok_or(CheckoutError::Unsafe("属性语法"))?;
        if pattern.starts_with("[attr]") || pattern.contains(['"', '\\', '!']) {
            return Err(CheckoutError::Unsafe("属性语法"));
        }
        if fields.any(|attribute| {
            !matches!(
                attribute,
                "text" | "text=auto" | "-text" | "binary" | "eol=lf" | "eol=crlf"
            )
        }) {
            return Err(CheckoutError::Unsafe("不允许的检出属性"));
        }
    }
    Ok(())
}

pub(super) fn parse_tree(bytes: &[u8], oid_size: usize) -> Result<Tree, CheckoutError> {
    let mut tree = Tree {
        files: BTreeMap::new(),
        directories: BTreeSet::new(),
        index: Vec::new(),
        attributes: Vec::new(),
    };
    if bytes.is_empty() {
        return Ok(tree);
    }
    let mut paths = BTreeMap::new();
    let body = bytes
        .strip_suffix(b"\0")
        .ok_or(CheckoutError::Unsafe("提交树记录缺少终止符"))?;
    let mut remaining = MAX_BYTES / 2;
    use std::io::Write;
    for (offset, record) in body.split(|byte| *byte == 0).enumerate() {
        if offset >= 30_000 {
            return Err(CheckoutError::Unsafe("提交树条目超限"));
        }
        let line =
            std::str::from_utf8(record).map_err(|_| CheckoutError::Unsafe("提交路径编码"))?;
        let (header, path) = line
            .split_once('\t')
            .ok_or(CheckoutError::Unsafe("无效提交树记录"))?;
        let mut fields = header.split_ascii_whitespace();
        let mode = match (fields.next(), fields.next()) {
            (Some(mode @ ("100644" | "100755")), Some("blob")) => mode,
            _ => {
                return Err(CheckoutError::Unsafe(
                    "提交包含 gitlink、符号链接或未知文件模式",
                ));
            }
        };
        let oid = fields
            .next()
            .filter(|oid| oid.len() == oid_size && hex(oid.as_bytes()))
            .ok_or(CheckoutError::Unsafe("树对象 OID"))?;
        let size = fields
            .next()
            .and_then(|size| size.parse::<u64>().ok())
            .filter(|_| fields.next().is_none())
            .ok_or(CheckoutError::Unsafe("缺失或无效的树对象大小"))?;
        remaining = remaining
            .checked_sub(size)
            .ok_or(CheckoutError::Unsafe("检出内容大小超限"))?;
        if path.len() > 1024
            || path.bytes().filter(|byte| *byte == b'/').count() >= 32
            || path.split('/').any(|name| !portable_component(name))
        {
            return Err(CheckoutError::Unsafe("提交路径不可安全表示"));
        }
        for (offset, _) in path.match_indices('/') {
            let directory = &path[..offset];
            remember_path(&mut paths, directory, true)?;
            tree.directories.insert(directory.to_owned());
        }
        remember_path(&mut paths, path, false)?;
        if let Some(name) = path
            .rsplit('/')
            .next()
            .filter(|name| name.eq_ignore_ascii_case(".gitattributes"))
        {
            if name != ".gitattributes" || size > MAX_CONFIG {
                return Err(CheckoutError::Unsafe("属性文件路径或大小"));
            }
            tree.attributes.push(oid.to_owned());
        }
        tree.files.insert(path.to_owned(), mode == "100755");
        write!(&mut tree.index, "{mode} {oid} 0\t{path}\0")?;
    }
    Ok(tree)
}

fn remember_path<'a>(
    paths: &mut BTreeMap<String, (&'a str, bool)>,
    path: &'a str,
    directory: bool,
) -> Result<(), CheckoutError> {
    use std::collections::btree_map::Entry;
    match paths.entry(path.to_ascii_lowercase()) {
        Entry::Occupied(entry) if directory && *entry.get() == (path, true) => Ok(()),
        Entry::Occupied(_) => Err(CheckoutError::Unsafe("提交路径别名或文件目录冲突")),
        Entry::Vacant(entry) => {
            entry.insert((path, directory));
            Ok(())
        }
    }
}

fn portable_component(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 255
        || !name.is_ascii()
        || name.ends_with(['.', ' '])
        || name.starts_with(' ')
        || name.bytes().any(|byte| {
            byte.is_ascii_control()
                || matches!(
                    byte,
                    b'<' | b'>' | b':' | b'"' | b'\\' | b'|' | b'?' | b'*' | b'~'
                )
        })
    {
        return false;
    }
    let folded = name.to_ascii_lowercase();
    let stem = folded.split('.').next().unwrap_or_default();
    !matches!(folded.as_str(), ".git")
        && !matches!(
            stem,
            "con"
                | "prn"
                | "aux"
                | "nul"
                | "com0"
                | "com1"
                | "com2"
                | "com3"
                | "com4"
                | "com5"
                | "com6"
                | "com7"
                | "com8"
                | "com9"
                | "lpt0"
                | "lpt1"
                | "lpt2"
                | "lpt3"
                | "lpt4"
                | "lpt5"
                | "lpt6"
                | "lpt7"
                | "lpt8"
                | "lpt9"
        )
}

fn hex(value: &[u8]) -> bool {
    value
        .iter()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

pub(super) fn check_cancel(cancel: &mut oneshot::Receiver<()>) -> Result<(), CheckoutError> {
    match cancel.try_recv() {
        Err(oneshot::error::TryRecvError::Empty) => Ok(()),
        _ => Err(CheckoutError::Cancelled),
    }
}

async fn run_child(
    command: &mut Command,
    cancel: &mut oneshot::Receiver<()>,
    timeout: Duration,
) -> Result<Output, CheckoutError> {
    check_cancel(cancel)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let result = {
        let stdout = child
            .stdout
            .take()
            .ok_or(GitError::InvalidMetadata("缺少 stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(GitError::InvalidMetadata("缺少 stderr"))?;
        let collect = async {
            let (status, stdout, stderr) = tokio::try_join!(
                async { child.wait().await.map_err(GitError::from) },
                bounded(stdout),
                bounded(stderr)
            )?;
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        };
        // 超时或取消时先释放读取 future 持有的两个管道。
        tokio::select! {
            biased;
            _ = cancel => Err(CheckoutError::Cancelled),
            result = tokio::time::timeout(timeout, collect) => match result {
                Ok(result) => result.map_err(CheckoutError::Git),
                Err(_) => Err(GitError::Timeout.into()),
            },
        }
    };
    if let Err(error) = result {
        let _ = child.start_kill();
        if !matches!(
            tokio::time::timeout(timeout.max(Duration::from_secs(2)), child.wait()).await,
            Ok(Ok(_))
        ) {
            let reason = match error {
                CheckoutError::Cancelled => "cancelled",
                CheckoutError::Git(GitError::Timeout) => "timeout",
                _ => "output",
            };
            return Err(CheckoutError::StopUnconfirmed(reason));
        }
        return Err(error);
    }
    result
}

async fn bounded(reader: impl AsyncRead + Unpin) -> Result<Vec<u8>, GitError> {
    let mut bytes = Vec::new();
    reader
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(GitError::OutputLimit);
    }
    Ok(bytes)
}

#[cfg(test)]
pub(super) mod tests;
