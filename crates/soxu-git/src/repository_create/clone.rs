//! 显式 Clone（首版仅本地源）：固定参数语义、独立对象复制与只读复核。

use std::ffi::OsStr;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use super::platform::{self, AuthorizedDirectory, DirectoryIdentity};
use super::sandbox::{self, Sandbox};
use super::{RepositoryCreateError, RepositoryCreateResult};

const CLONE_ARGS: [&str; 5] = [
    "clone",
    "--no-local",
    "--no-checkout",
    "--no-recurse-submodules",
    "--template=",
];

/// 已核验的本地来源：路径用于重叠判定，句柄用于快照与同一性复核。
#[derive(Debug)]
pub(super) struct SourceFacts {
    pub root: PathBuf,
    pub directory: AuthorizedDirectory,
    pub git_dir: AuthorizedDirectory,
}

/// 核验本地来源：拒绝 bare、关联工作树、UNC、链接、shallow、alternates、promisor 与外部过滤器。
pub(super) fn verify_source(source_root: &Path) -> Result<SourceFacts, RepositoryCreateError> {
    if !super::target::local_absolute(source_root) {
        return Err(RepositoryCreateError::UnsupportedSource(
            "来源必须是本地绝对路径",
        ));
    }
    let directory = AuthorizedDirectory::open(source_root)?;
    // `.git` 从源根句柄派生，不做路径重开；链接与 reparse 由句柄链拒绝。
    let git_dir = directory.child(OsStr::new(".git"))?;
    // 源根与源 .git 的挂载类型也必须属于受支持的本地文件系统。
    super::platform::require_supported_filesystem(&directory)?;
    super::platform::require_supported_filesystem(&git_dir)?;
    // 结构判定全部在句柄锚定路径下完成，与后续快照复制读取同一对象。
    drop(super::platform::open_regular_anchored(
        &git_dir,
        OsStr::new("HEAD"),
    )?);
    let _objects = git_dir.child(OsStr::new("objects"))?;
    let _refs = git_dir.child(OsStr::new("refs"))?;
    for name in ["commondir", "gitdir", "shallow"] {
        if anchored(&git_dir, name)?.exists() {
            return Err(RepositoryCreateError::UnsupportedSource(
                "来源包含未支持的工作树或浅克隆元数据",
            ));
        }
    }
    let config = read_anchored(&git_dir, "config")?;
    super::config_scan::reject_unsafe(&config, "来源仓库配置")?;
    Ok(SourceFacts {
        root: directory.path().to_owned(),
        directory,
        git_dir,
    })
}

fn anchored(directory: &AuthorizedDirectory, name: &str) -> Result<PathBuf, RepositoryCreateError> {
    Ok(directory.anchor()?.join(name))
}

fn read_anchored(
    directory: &AuthorizedDirectory,
    name: &str,
) -> Result<Vec<u8>, RepositoryCreateError> {
    const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
    let mut file = super::platform::open_regular_anchored(directory, OsStr::new(name))?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(RepositoryCreateError::Io)?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(RepositoryCreateError::UnsupportedSource(
            "来源配置超过长度上限",
        ));
    }
    Ok(bytes)
}
/// 允许进入快照的源 `.git` 顶层条目；未列出的条目一律拒绝，避免隐式携带额外 Git 状态。
const SNAPSHOT_LAYOUT: [&str; 13] = [
    "HEAD",
    "config",
    "packed-refs",
    "refs",
    "objects",
    "info",
    "branches",
    "description",
    "hooks",
    "logs",
    "index",
    "COMMIT_EDITMSG",
    "ORIG_HEAD",
];
const SNAPSHOT_MAX_ENTRIES: usize = 200_000;
const SNAPSHOT_MAX_FILE: u64 = 64 * 1024 * 1024;
const SNAPSHOT_MAX_TOTAL: u64 = 512 * 1024 * 1024;

#[derive(Debug, Default)]
struct SnapshotBudget {
    entries: usize,
    bytes: u64,
}

impl SnapshotBudget {
    fn count(&mut self) -> Result<(), RepositoryCreateError> {
        if self.entries >= SNAPSHOT_MAX_ENTRIES {
            return Err(RepositoryCreateError::UnsupportedSource(
                "来源对象库条目超过快照预算",
            ));
        }
        self.entries += 1;
        Ok(())
    }

    fn copy_file(
        &mut self,
        source: &AuthorizedDirectory,
        name: &OsStr,
        destination: &Path,
    ) -> Result<(), RepositoryCreateError> {
        self.count()?;
        let mut input = super::platform::open_regular_anchored(source, name)?;
        let remaining = SNAPSHOT_MAX_TOTAL.saturating_sub(self.bytes);
        let limit = SNAPSHOT_MAX_FILE.min(remaining);
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(RepositoryCreateError::Io)?;
        let copied = std::io::copy(
            &mut std::io::Read::by_ref(&mut input).take(limit),
            &mut output,
        )
        .map_err(RepositoryCreateError::Io)?;
        let mut probe = [0_u8; 1];
        if std::io::Read::read(&mut input, &mut probe).map_err(RepositoryCreateError::Io)? > 0 {
            return Err(RepositoryCreateError::UnsupportedSource(
                "来源文件超过快照预算",
            ));
        }
        output.sync_all().map_err(RepositoryCreateError::Io)?;
        self.bytes += copied;
        Ok(())
    }
}

fn copy_tree(
    source: &AuthorizedDirectory,
    destination: &AuthorizedDirectory,
    budget: &mut SnapshotBudget,
) -> Result<(), RepositoryCreateError> {
    let anchor = source.anchor()?;
    let mut reader = std::fs::read_dir(&anchor).map_err(RepositoryCreateError::Io)?;
    while let Some(entry) = reader
        .next()
        .transpose()
        .map_err(RepositoryCreateError::Io)?
    {
        let name = entry.file_name();
        let path = anchor.join(&name);
        let metadata = std::fs::symlink_metadata(&path).map_err(RepositoryCreateError::Io)?;
        if metadata.is_symlink() || platform::is_reparse(&metadata) {
            return Err(RepositoryCreateError::UnsupportedSource(
                "来源包含链接或 reparse point",
            ));
        }
        if metadata.is_dir() {
            budget.count()?;
            let child_source = source.child(&name)?;
            let child_destination = destination.create_child(&name)?;
            copy_tree(&child_source, &child_destination, budget)?;
        } else if metadata.is_file() {
            budget.copy_file(source, &name, &destination.anchor()?.join(&name))?;
        } else {
            return Err(RepositoryCreateError::UnsupportedSource("来源包含特殊文件"));
        }
    }
    Ok(())
}

/// 在宿主私有暂存内建立源 `.git` 的私有快照；返回持有句柄与交给 Git 的路径。
///
/// 顶层条目按白名单逐一复制，目录与文件都经句柄链定位；捕获前后各核一次源身份，
/// 捕获完成后子进程只读快照，不回读原源。
pub(super) fn snapshot(
    sandbox: &Sandbox,
    facts: &SourceFacts,
) -> Result<(AuthorizedDirectory, PathBuf), RepositoryCreateError> {
    facts.directory.refresh()?;
    facts.git_dir.refresh()?;
    let anchor = facts.git_dir.anchor()?;
    let mut names = Vec::new();
    let mut reader = std::fs::read_dir(&anchor).map_err(RepositoryCreateError::Io)?;
    while let Some(entry) = reader
        .next()
        .transpose()
        .map_err(RepositoryCreateError::Io)?
    {
        let name = entry.file_name();
        let Some(text) = name.to_str() else {
            return Err(RepositoryCreateError::UnsupportedSource(
                "来源布局包含不可表示条目",
            ));
        };
        if !SNAPSHOT_LAYOUT.contains(&text) {
            return Err(RepositoryCreateError::UnsupportedSource(
                "来源布局包含未支持的顶层条目",
            ));
        }
        names.push(name);
    }
    let root = sandbox.work().create_child(OsStr::new("source"))?;
    let mut budget = SnapshotBudget::default();
    for name in names {
        let path = anchor.join(&name);
        let metadata = std::fs::symlink_metadata(&path).map_err(RepositoryCreateError::Io)?;
        if metadata.is_symlink() || platform::is_reparse(&metadata) {
            return Err(RepositoryCreateError::UnsupportedSource(
                "来源包含链接或 reparse point",
            ));
        }
        if metadata.is_dir() {
            budget.count()?;
            let child_source = facts.git_dir.child(&name)?;
            let child_destination = root.create_child(&name)?;
            copy_tree(&child_source, &child_destination, &mut budget)?;
        } else if metadata.is_file() {
            budget.copy_file(&facts.git_dir, &name, &root.anchor()?.join(&name))?;
        } else {
            return Err(RepositoryCreateError::UnsupportedSource("来源包含特殊文件"));
        }
    }
    // 快照内实际检查：配置白名单与外部对象引用，避免用源路径做出与 Git 读取不一致的判断。
    let config = read_anchored(&root, "config")?;
    super::config_scan::reject_unsafe(&config, "来源仓库配置")?;
    let objects = root.child(OsStr::new("objects"))?;
    if anchored(&objects, "info")?.join("alternates").exists() {
        return Err(RepositoryCreateError::UnsupportedSource(
            "来源依赖外部对象库",
        ));
    }
    facts.directory.refresh()?;
    facts.git_dir.refresh()?;
    let path = root.child_process_path()?;
    Ok((root, path))
}
/// 运行固定参数语义的 `git clone`；目标必须是宿主私有暂存路径。
pub(super) async fn run(
    sandbox: &Sandbox,
    program: &Path,
    snapshot: &Path,
    target: &Path,
    origin: &Path,
) -> Result<RepositoryCreateResult, RepositoryCreateError> {
    let source = platform::native_path(snapshot)?;
    let destination = platform::native_path(target)?;
    let mut command = sandbox.command(program, sandbox.root());
    command
        .args(CLONE_ARGS)
        .arg("--config")
        .arg("core.hooksPath=")
        .arg("--")
        .arg(&source)
        .arg(&destination);
    let result = super::init::outcome(
        sandbox::run(
            &mut command,
            sandbox::OUTPUT_LIMIT,
            sandbox::COMMAND_TIMEOUT,
        )
        .await,
    )?;
    if result != RepositoryCreateResult::Succeeded {
        return Ok(result);
    }
    // 快照路径是进程级句柄锚定，不能成为持久远端地址：改写为已授权源路径。
    let origin = platform::native_path(origin)?;
    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(&destination)
        .args(["config", "--local", "remote.origin.url"])
        .arg(&origin);
    Ok(
        match sandbox::run(
            &mut command,
            sandbox::OUTPUT_LIMIT,
            sandbox::COMMAND_TIMEOUT,
        )
        .await
        {
            Ok(output) if output.status.success() => RepositoryCreateResult::Succeeded,
            Ok(_) => RepositoryCreateResult::Unknown {
                stop_confirmed: true,
                reason: "无法规范化克隆远端地址".to_owned(),
            },
            Err(failure) => RepositoryCreateResult::Unknown {
                stop_confirmed: failure.direct_child_reaped,
                reason: "无法规范化克隆远端地址".to_owned(),
            },
        },
    )
}

/// 克隆后只读复核与受控发布结果。
#[derive(Debug, Default)]
pub(super) struct CloneObservation {
    pub git_dir_present: bool,
    pub git_dir_identity: Option<DirectoryIdentity>,
    pub head_branch: Option<String>,
    pub head_oid: Option<String>,
    /// 确证 unborn：HEAD 是分支引用且该分支确实不存在。
    pub unborn: bool,
    pub published_entries: usize,
    pub notes: Vec<String>,
}

impl CloneObservation {
    /// 确证空仓库：只有 `probe_head` 返回 Unborn 时为真。
    pub(super) fn empty_remote(&self) -> bool {
        self.unborn
    }
}

/// 只读复核克隆布局并解析远端 HEAD；不会创建、覆盖或删除任何文件。
pub(super) async fn inspect(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    holder: Option<&super::platform::AuthorizedDirectory>,
) -> Result<CloneObservation, RepositoryCreateError> {
    let mut observation = CloneObservation::default();
    let git_dir = target.join(".git");
    match std::fs::symlink_metadata(&git_dir) {
        Ok(metadata) => {
            if !platform::plain_directory(&metadata) {
                return Err(RepositoryCreateError::UnsafeContent(
                    "克隆产物 .git 不是受控实体目录",
                ));
            }
            observation.git_dir_present = true;
            observation.git_dir_identity = Some(match holder {
                Some(handle) => handle.child(std::ffi::OsStr::new(".git"))?.identity(),
                None => AuthorizedDirectory::open(&git_dir)?.identity(),
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            observation.notes.push("目标下没有 .git 目录".to_owned());
            return Ok(observation);
        }
        Err(error) => return Err(RepositoryCreateError::Io(error)),
    }
    if git_dir.join("shallow").exists() {
        return Err(RepositoryCreateError::UnsafeContent("克隆产物是浅克隆"));
    }
    if git_dir
        .join("objects")
        .join("info")
        .join("alternates")
        .exists()
    {
        return Err(RepositoryCreateError::UnsafeContent(
            "克隆产物依赖外部对象库",
        ));
    }
    let config = std::fs::read(git_dir.join("config")).map_err(RepositoryCreateError::Io)?;
    super::config_scan::reject_unsafe(&config, "克隆产物配置")?;

    match super::checkout::probe_head(sandbox, program, target).await? {
        super::checkout::HeadState::Commit(oid) => {
            observation.head_oid = Some(oid);
        }
        super::checkout::HeadState::Unborn(branch) => {
            observation.unborn = true;
            observation.head_branch = Some(branch);
            observation
                .notes
                .push("远端为空仓库，已克隆但没有可检出提交".to_owned());
            return Ok(observation);
        }
        super::checkout::HeadState::Unverifiable(reason) => {
            return Err(RepositoryCreateError::UnsafeContent(reason));
        }
    }
    let oid = observation
        .head_oid
        .clone()
        .ok_or(RepositoryCreateError::UnsafeContent("缺少已解析的 HEAD"))?;
    observation.head_branch = capture(sandbox, program, target, &["symbolic-ref", "-q", "HEAD"])
        .await?
        .map(|stdout| String::from_utf8_lossy(&stdout).trim().to_owned());
    if let Some(branch) = observation.head_branch.as_deref()
        && !branch.starts_with("refs/heads/")
    {
        return Err(RepositoryCreateError::UnsafeContent(
            "远端 HEAD 指向非分支引用",
        ));
    }
    let sized = oid.len() == 40 || oid.len() == 64;
    if !sized || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RepositoryCreateError::UnsafeContent(
            "HEAD 不是完整提交 OID",
        ));
    }
    observation
        .notes
        .push("已解析远端 HEAD，工作树与 index 尚未发布".to_owned());
    Ok(observation)
}

/// 在宿主私有暂存内完整预检提交树与属性，再发布工作树并建立匹配 HEAD 的 index。
/// 该函数只写 `target`（暂存仓库根），不碰最终目标。
pub(super) async fn stage(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    holder: Option<&super::platform::AuthorizedDirectory>,
) -> Result<CloneObservation, RepositoryCreateError> {
    let mut observation = inspect(sandbox, program, target, holder).await?;
    let Some(oid) = observation.head_oid.clone() else {
        return Ok(observation);
    };
    let entries = super::checkout::precheck_tree(sandbox, program, target, &oid).await?;
    observation.published_entries =
        super::checkout::publish_tree(sandbox, program, target, &oid, &entries).await?;
    Ok(observation)
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
