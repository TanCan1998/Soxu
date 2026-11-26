use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use super::git::{self, Git, strings};
use super::platform::{self, Directory, Entry};
use super::record::Operation;
use super::*;
use crate::GitProbe;

#[derive(Clone, PartialEq, Eq)]
struct Candidate {
    path: Vec<u8>,
    mode: Option<String>,
    base: bool,
    omission: Option<OmissionReason>,
}
struct Captured {
    path: Vec<u8>,
    oid: Option<String>,
    identity: Option<String>,
}

pub(super) async fn capture(
    probe: &GitProbe,
    directory: &Path,
    options: &CheckpointOptions,
    observe: &mut impl FnMut(CheckpointStage),
) -> Result<CheckpointRecord, CheckpointError> {
    platform::platform_observation()?;
    validate_policy(&options.policy)?;
    let input = Directory::open(directory)?;
    let state = super::record::state_directory(&options.state_directory)?;
    let root_path = input.path().to_owned();
    let discovery_index = state.path().join(format!("absent-{}", git::id()));
    let deadline = Instant::now() + options.policy.total_timeout;
    let discovery = Git {
        probe,
        root: &root_path,
        index: &discovery_index,
        hooks: state.path(),
        deadline,
        max_output: options.policy.max_output_bytes,
        timeout: options.policy.command_timeout,
    };
    let initial = observation(&discovery, &input).await?;
    let state_path = fs::canonicalize(state.path())?;
    if [
        &initial.worktree_root,
        &initial.git_dir,
        &initial.common_dir,
    ]
    .iter()
    .any(|dir| within(&state_path, dir))
    {
        return Err(CheckpointError::Invalid(
            "应用状态目录不能位于工作树或 Git 元数据内",
        ));
    }
    if fs::canonicalize(input.path())? != initial.worktree_root {
        return Err(CheckpointError::Invalid("捕获入口必须是工作树根"));
    }
    let operation = Operation::new(state)?;
    let git = Git {
        deadline,
        ..operation.git(probe, &root_path, &options.policy)
    };
    let before = observation(&git, &input).await?;
    if before != initial {
        return Err(CheckpointError::Drift);
    }
    let _lock = operation.lock(
        &git.digest(
            &platform::git_name(root_path.as_os_str())
                .ok_or(CheckpointError::Invalid("工作树路径编码"))?,
        )
        .await?,
    )?;
    let admin = Directory::open(&before.git_dir)?;
    let common = Directory::open(&before.common_dir)?;
    validate_repository(&git, &admin).await?;
    reject_reflogs(&git, &common).await?;
    let parent = parent_oid(&git).await?;
    if let Some(parent) = &parent {
        git.text(&["read-tree", parent]).await?;
    } else {
        git.text(&["read-tree", "--empty"]).await?;
    }
    let base_tree = git::oid(&git.text(&["write-tree"]).await?)?;
    let candidates = enumerate(&git, &input, &admin, parent.as_deref(), &options.policy).await?;
    let mut coverage = CoverageManifest {
        complete: true,
        included: Vec::new(),
        omissions: Vec::new(),
        captured_bytes: 0,
        policy_version: policy_version(&options.policy),
    };
    let mut examined_bytes = 0;
    let mut captured = Vec::new();
    let mut attributes = Vec::new();
    for candidate in &candidates {
        if Instant::now() >= git.deadline {
            return Err(CheckpointError::Timeout);
        }
        if let Some(reason) = candidate.omission {
            omit(&mut coverage, candidate, reason);
            continue;
        }
        let attrs = attributes_for(&git, &candidate.path).await?;
        attributes.push((candidate.path.clone(), attrs.clone()));
        if let Some(reason) = transformation(&attrs) {
            omit(&mut coverage, candidate, reason);
            continue;
        }
        let relative = safe_path(&candidate.path)?;
        match input.entry(&relative)? {
            Entry::Absent => {
                // 路径枚举后消失的 untracked 也是漂移；只有 tracked 可产生删除 entry。
                if candidate.mode.is_none() {
                    return Err(CheckpointError::Drift);
                }
                remove_entry(&git, &candidate.path).await?;
                captured.push(Captured {
                    path: candidate.path.clone(),
                    oid: None,
                    identity: None,
                });
                coverage.included.push(CoveredPath {
                    path: candidate.path.clone(),
                    blob_oid: None,
                    bytes: 0,
                    warning: false,
                    representation: "deleted".into(),
                });
            }
            Entry::Special => omit(&mut coverage, candidate, OmissionReason::SpecialFile),
            Entry::Regular(mut file) => {
                let hard_limit = options
                    .policy
                    .max_binary_bytes
                    .max(options.policy.max_text_bytes);
                let size = file.metadata()?.len();
                if size > hard_limit {
                    omit(&mut coverage, candidate, OmissionReason::IndividualBudget);
                    continue;
                }
                let remaining = options.policy.max_total_bytes - examined_bytes;
                if size > remaining {
                    omit(&mut coverage, candidate, OmissionReason::TotalBudget);
                    continue;
                }
                let (bytes, identity) =
                    platform::read_bounded(&mut file, hard_limit.min(remaining))?
                        .ok_or(CheckpointError::Drift)?;
                examined_bytes += bytes.len() as u64;
                let binary = binary(&bytes);
                let omission = if sensitive_content(&bytes) {
                    Some(OmissionReason::SensitiveContent)
                } else if binary && !options.policy.allow_binary {
                    Some(OmissionReason::BinaryNotAuthorized)
                } else if binary && bytes.len() as u64 > options.policy.max_binary_bytes {
                    Some(OmissionReason::BinaryBudget)
                } else if !binary && bytes.len() as u64 > options.policy.max_text_bytes {
                    Some(OmissionReason::TextBudget)
                } else {
                    None
                };
                if let Some(reason) = omission {
                    omit(&mut coverage, candidate, reason);
                    continue;
                }
                let expected = git.digest(&bytes).await?;
                // Git 只从固定 File 的 stdin 得到字节，不接触工作树文件路径。
                let blob = git.hash_file(&mut file, &bytes).await?;
                let (_, after_identity) = {
                    use std::io::Seek;
                    file.rewind()?;
                    platform::read_bounded(&mut file, hard_limit)?.ok_or(CheckpointError::Drift)?
                };
                if expected != blob || identity != after_identity {
                    return Err(CheckpointError::Drift);
                }
                let mode =
                    if platform::executable(&file, candidate.mode.as_deref() == Some("100755"))? {
                        "100755"
                    } else {
                        "100644"
                    };
                set_entry(&git, mode, &blob, &candidate.path).await?;
                coverage.captured_bytes += bytes.len() as u64;
                coverage.included.push(CoveredPath {
                    path: candidate.path.clone(),
                    blob_oid: Some(blob.clone()),
                    bytes: bytes.len() as u64,
                    warning: bytes.len() as u64 >= options.policy.warning_bytes,
                    representation: "raw-blob".into(),
                });
                captured.push(Captured {
                    path: candidate.path.clone(),
                    oid: Some(blob),
                    identity: Some(identity),
                });
            }
        }
    }
    let tree = git::oid(&git.text(&["write-tree"]).await?)?;
    let changed = git
        .text(&[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "--no-renames",
            "--no-ext-diff",
            "-r",
            "-z",
            &base_tree,
            &tree,
            "--",
        ])
        .await?
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .count();
    let mut args = strings(&["commit-tree", &tree]);
    if let Some(parent) = &parent {
        args.extend(strings(&["-p", parent]));
    }
    let commit = git::oid(
        &git.run(&args, Some(b"Soxu private round checkpoint\n"))
            .await?,
    )?;
    observe(CheckpointStage::Captured);
    verify_capture(
        &git,
        &input,
        &admin,
        &common,
        &before,
        &candidates,
        &captured,
        &attributes,
        parent.as_deref(),
        &options.policy,
    )
    .await?;
    observe(CheckpointStage::Revalidated);
    let after = observation(&git, &input).await?;
    if before != after {
        return Err(CheckpointError::Drift);
    }
    let mut record = CheckpointRecord {
        schema_version: 1,
        workspace_id: options.scope.workspace.clone(),
        session_id: options.scope.session.clone(),
        checkpoint_id: operation.id.clone(),
        turn_index: options.turn_index,
        parent_oid: parent,
        base_tree_oid: base_tree,
        worktree_tree_oid: tree,
        commit_oid: commit,
        changed_files: u32::try_from(changed).map_err(|_| CheckpointError::Budget("变更数量"))?,
        before,
        after,
        coverage,
        created_at: chrono::Utc::now().to_rfc3339(),
        lifecycle: CheckpointLifecycle::Intent,
    };
    git.text(&["check-ref-format", &record.reference()]).await?;
    operation.intent(&record)?;
    observe(CheckpointStage::IntentDurable);
    verify_capture(
        &git,
        &input,
        &admin,
        &common,
        &record.before,
        &candidates,
        &captured,
        &attributes,
        record.parent_oid.as_deref(),
        &options.policy,
    )
    .await?;
    reject_reflogs(&git, &common).await?;
    verify_ref_directories(&common, &record)?;
    let zero = "0".repeat(record.commit_oid.len());
    if let Err(error) = git
        .text(&[
            "update-ref",
            "--no-deref",
            &record.reference(),
            &record.commit_oid,
            &zero,
        ])
        .await
    {
        return match error {
            CheckpointError::Git { .. } => Err(CheckpointError::CasConflict),
            other => Err(other),
        };
    }
    reject_reflogs(&git, &common).await?;
    let resolved = git::oid(
        &git.text(&["rev-parse", "--verify", &record.reference()])
            .await?,
    )?;
    if resolved != record.commit_oid {
        return Err(CheckpointError::CasConflict);
    }
    record.lifecycle = CheckpointLifecycle::Published;
    operation.publish(&record)?;
    Ok(record)
}

fn within(path: &Path, directory: &Path) -> bool {
    #[cfg(windows)]
    return Path::new(&path.to_string_lossy().to_lowercase())
        .starts_with(Path::new(&directory.to_string_lossy().to_lowercase()));
    #[cfg(not(windows))]
    path.starts_with(directory)
}

fn validate_policy(p: &CheckpointPolicy) -> Result<(), CheckpointError> {
    if p.max_paths == 0
        || p.max_paths > 10_000
        || p.max_binary_bytes > 32 * 1024 * 1024
        || p.max_text_bytes > 8 * 1024 * 1024
        || p.max_total_bytes > 64 * 1024 * 1024
        || p.max_output_bytes > 32 * 1024 * 1024
        || p.max_output_bytes == 0
        || p.total_timeout.is_zero()
        || p.total_timeout > std::time::Duration::from_secs(600)
        || p.command_timeout.is_zero()
        || p.command_timeout > std::time::Duration::from_secs(60)
    {
        return Err(CheckpointError::Invalid("预算超过首版硬上限"));
    }
    Ok(())
}

fn policy_version(p: &CheckpointPolicy) -> String {
    format!(
        "soxu-checkpoint-v1;warning={};text={};binary={};total={};paths={};allow_binary={}",
        p.warning_bytes,
        p.max_text_bytes,
        p.max_binary_bytes,
        p.max_total_bytes,
        p.max_paths,
        p.allow_binary
    )
}

async fn parent_oid(git: &Git<'_>) -> Result<Option<String>, CheckpointError> {
    let bytes = git
        .accepted(
            &strings(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]),
            None,
            &[1],
        )
        .await?;
    if bytes.is_empty() {
        if git.text(&["symbolic-ref", "-q", "HEAD"]).await?.is_empty() {
            return Err(CheckpointError::Invalid("无效 unborn HEAD"));
        }
        Ok(None)
    } else {
        Ok(Some(git::oid(&bytes)?))
    }
}

pub(super) async fn repository_paths(
    git: &Git<'_>,
    root: &Directory,
) -> Result<(PathBuf, PathBuf, PathBuf, Vec<String>), CheckpointError> {
    root.verify()?;
    if git.text(&["rev-parse", "--is-bare-repository"]).await? != b"false\n" {
        return Err(CheckpointError::Invalid("bare 仓库"));
    }
    let locator = root.anchored().join(".git");
    let marker = platform::metadata_if_present(&locator)?
        .ok_or(CheckpointError::Invalid("工作树根缺少 .git"))?;
    if !platform::is_plain_directory(&marker)
        && !matches!(platform::open_entry(&locator)?, Entry::Regular(_))
    {
        return Err(CheckpointError::Invalid("工作树 .git 定位文件不安全"));
    }
    let resolve = |bytes: Vec<u8>| -> Result<Directory, CheckpointError> {
        let bytes = bytes
            .strip_suffix(b"\n")
            .ok_or(CheckpointError::Invalid("路径行结束"))?;
        let path = platform::native_component(bytes).ok_or(CheckpointError::Invalid("路径编码"))?;
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(CheckpointError::Invalid("Git 未返回绝对路径"));
        }
        Ok(Directory::open(&path)?)
    };
    let actual_root = resolve(
        git.text(&["rev-parse", "--path-format=absolute", "--show-toplevel"])
            .await?,
    )?;
    let git_dir = resolve(
        git.text(&["rev-parse", "--path-format=absolute", "--absolute-git-dir"])
            .await?,
    )?;
    let common_dir = resolve(
        git.text(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .await?,
    )?;
    let mut identity = Vec::new();
    for dir in [&actual_root, &git_dir, &common_dir] {
        identity.push("directory".into());
        identity.extend(dir.identity()?);
    }
    Ok((
        fs::canonicalize(actual_root.path())?,
        fs::canonicalize(git_dir.path())?,
        fs::canonicalize(common_dir.path())?,
        identity,
    ))
}

async fn observation(
    git: &Git<'_>,
    root: &Directory,
) -> Result<CheckpointObservation, CheckpointError> {
    let (worktree_root, git_dir, common_dir, directory_identity) =
        repository_paths(git, root).await?;
    let mut head = git
        .accepted(&strings(&["symbolic-ref", "-q", "HEAD"]), None, &[1])
        .await?;
    head.push(0);
    if let Some(parent) = parent_oid(git).await? {
        head.extend_from_slice(parent.as_bytes());
    }
    let admin = Directory::open(&git_dir)?;
    let index_digest = match admin.entry(Path::new("index"))? {
        Entry::Absent => None,
        Entry::Special => return Err(CheckpointError::Invalid("主 index 非普通文件")),
        Entry::Regular(mut file) => {
            let (bytes, _) = platform::read_bounded(&mut file, 64 * 1024 * 1024)?
                .ok_or(CheckpointError::Budget("主 index"))?;
            Some(git.digest(&bytes).await?)
        }
    };
    let mut config = git
        .text(&[
            "config",
            "--local",
            "--null",
            "--list",
            "--includes",
            "--show-origin",
        ])
        .await?;
    let worktree = git
        .accepted(
            &strings(&[
                "config",
                "--local",
                "--type=bool",
                "--get",
                "extensions.worktreeConfig",
            ]),
            None,
            &[1],
        )
        .await?;
    if worktree == b"true\n" {
        config.extend(
            git.text(&[
                "config",
                "--worktree",
                "--null",
                "--list",
                "--includes",
                "--show-origin",
            ])
            .await?,
        );
    }
    Ok(CheckpointObservation {
        worktree_root,
        git_dir,
        common_dir,
        directory_identity,
        head,
        index_digest,
        configuration_digest: git.digest(&config).await?,
        platform: platform::platform_observation()?.into(),
    })
}

async fn validate_repository(git: &Git<'_>, admin: &Directory) -> Result<(), CheckpointError> {
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "REBASE_HEAD",
        "rebase-apply",
        "rebase-merge",
        "sequencer",
        "index.lock",
    ] {
        if platform::metadata_if_present(&admin.anchored().join(marker))?.is_some() {
            return Err(CheckpointError::Invalid("工作树有未完成的 Git 操作"));
        }
    }
    for key in ["core.sparseCheckout", "core.splitIndex"] {
        let args = strings(&["config", "--local", "--type=bool", "--get", key]);
        let value = git.accepted(&args, None, &[1]).await?;
        if value == b"true\n" {
            return Err(CheckpointError::Invalid("不支持 sparse/split index"));
        }
    }
    // 只读主 index；不能用绑定了私有 GIT_INDEX_FILE 的 ls-files 判断真实冲突。
    let main_index = admin.path().join("index");
    if platform::metadata_if_present(&main_index)?.is_some() {
        let main_git = Git {
            index: &main_index,
            ..*git
        };
        if !main_git
            .text(&["rev-parse", "--shared-index-path"])
            .await?
            .is_empty()
        {
            return Err(CheckpointError::Invalid("不支持 split index"));
        }
        if !main_git
            .text(&["ls-files", "--unmerged", "-z"])
            .await?
            .is_empty()
        {
            return Err(CheckpointError::Invalid("Unmerged index"));
        }
        let tagged = main_git.text(&["ls-files", "--sparse", "-v", "-z"]).await?;
        if tagged.split(|b| *b == 0).any(|s| {
            s.first()
                .is_some_and(|c| *c == b'S' || c.is_ascii_lowercase())
        }) {
            return Err(CheckpointError::Invalid(
                "不支持 skip-worktree/assume-unchanged",
            ));
        }
    }
    Ok(())
}

async fn reject_reflogs(git: &Git<'_>, common: &Directory) -> Result<(), CheckpointError> {
    if let Some(logs) = child_if_present(common, "logs")?
        && let Some(refs) = child_if_present(&logs, "refs")?
        && platform::metadata_if_present(&refs.anchored().join("soxu"))?.is_some()
    {
        return Err(CheckpointError::Invalid("私有命名空间已有意外 reflog"));
    }
    let listed = git.text(&["reflog", "list"]).await?;
    if listed
        .split(|byte| *byte == 10)
        .any(|name| name.starts_with(b"refs/soxu/"))
    {
        return Err(CheckpointError::Invalid("私有命名空间已有意外 reflog"));
    }
    Ok(())
}

fn verify_ref_directories(
    common: &Directory,
    record: &CheckpointRecord,
) -> Result<(), CheckpointError> {
    let Some(mut directory) = child_if_present(common, "refs")? else {
        return Ok(());
    };
    for name in [
        "soxu",
        "checkpoints",
        record.workspace_id.as_str(),
        record.session_id.as_str(),
    ] {
        let Some(child) = child_if_present(&directory, name)? else {
            break;
        };
        directory = child;
    }
    Ok(())
}

fn child_if_present(parent: &Directory, name: &str) -> Result<Option<Directory>, CheckpointError> {
    match parent.child(name.as_ref()) {
        Ok(directory) => Ok(Some(directory)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

async fn enumerate(
    git: &Git<'_>,
    root: &Directory,
    admin: &Directory,
    parent: Option<&str>,
    p: &CheckpointPolicy,
) -> Result<Vec<Candidate>, CheckpointError> {
    let mut paths = BTreeMap::<Vec<u8>, Candidate>::new();
    let tree = if let Some(parent) = parent {
        git.text(&["ls-tree", "-r", "-z", "--full-tree", parent])
            .await?
    } else {
        Vec::new()
    };
    for entry in tree.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let at = entry
            .iter()
            .position(|b| *b == b'\t')
            .ok_or(CheckpointError::Invalid("tree 条目"))?;
        let path = entry[at + 1..].to_vec();
        safe_path(&path)?;
        let mode = std::str::from_utf8(&entry[..6])
            .map_err(|_| CheckpointError::Invalid("mode"))?
            .to_owned();
        let omission = if mode != "100644" && mode != "100755" {
            Some(OmissionReason::InheritedSpecial)
        } else if sensitive_path(&path) {
            Some(OmissionReason::SensitivePath)
        } else {
            None
        };
        paths.insert(
            path.clone(),
            Candidate {
                path,
                mode: Some(mode),
                base: true,
                omission,
            },
        );
    }
    // 主 index 中新增而 HEAD 尚未跟踪的路径也必须纳入隔离捕获。
    let index = admin.path().join("index");
    if platform::metadata_if_present(&admin.anchored().join("index"))?.is_some() {
        let main_git = Git {
            index: &index,
            ..*git
        };
        let tracked = main_git.text(&["ls-files", "--stage", "-z"]).await?;
        for entry in tracked.split(|b| *b == 0).filter(|s| !s.is_empty()) {
            let at = entry
                .iter()
                .position(|b| *b == b'\t')
                .ok_or(CheckpointError::Invalid("index 条目"))?;
            let path = entry[at + 1..].to_vec();
            safe_path(&path)?;
            let mode = String::from_utf8(entry[..6].to_vec())
                .map_err(|_| CheckpointError::Invalid("mode"))?;
            paths.entry(path.clone()).or_insert(Candidate {
                omission: if mode != "100644" && mode != "100755" {
                    Some(OmissionReason::InheritedSpecial)
                } else if sensitive_path(&path) {
                    Some(OmissionReason::SensitivePath)
                } else {
                    None
                },
                path,
                mode: Some(mode),
                base: false,
            });
        }
    }

    let mut queue = vec![(Directory::open(root.path())?, Vec::<u8>::new())];
    let mut visited = 0;
    while let Some((handle, prefix)) = queue.pop() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(handle.anchored())? {
            visited += 1;
            if visited > MAX_ENUMERATED_PATHS {
                return Err(CheckpointError::Budget("枚举路径数量超出硬上限"));
            }
            let entry = entry?;
            let name = platform::git_name(&entry.file_name())
                .ok_or(CheckpointError::Invalid("文件名编码"))?;
            entries.push(name);
        }
        entries.sort();
        for name in entries {
            if name.eq_ignore_ascii_case(b".git") {
                continue;
            }
            let mut path = prefix.clone();
            if !path.is_empty() {
                path.push(b'/');
            }
            path.extend(name);
            let relative = safe_path(&path)?;
            if paths.contains_key(&path) {
                continue;
            }
            let leaf = relative
                .file_name()
                .ok_or(CheckpointError::Invalid("路径没有末段"))?;
            let target = handle.anchored().join(leaf);
            let meta = fs::symlink_metadata(&target)?;
            let is_dir = platform::is_plain_directory(&meta);
            let child = if is_dir && !sensitive_path(&path) && !ignored(git, &path, true).await? {
                Some(handle.child(leaf)?)
            } else {
                None
            };
            let reason = if sensitive_path(&path) {
                Some(OmissionReason::SensitivePath)
            } else if ignored(git, &path, is_dir).await? {
                Some(OmissionReason::Ignored)
            } else if let Some(child) = &child
                && platform::metadata_if_present(&child.anchored().join(".git"))?.is_some()
            {
                Some(OmissionReason::NestedRepository)
            } else if !is_dir && !meta.is_file() {
                Some(OmissionReason::SpecialFile)
            } else {
                None
            };
            if let Some(reason) = reason {
                paths.insert(
                    path.clone(),
                    Candidate {
                        path,
                        mode: None,
                        base: false,
                        omission: Some(reason),
                    },
                );
            } else if let Some(child) = child {
                queue.push((child, path));
            } else {
                paths.insert(
                    path.clone(),
                    Candidate {
                        path,
                        mode: None,
                        base: false,
                        omission: None,
                    },
                );
            }
            if paths.len() > MAX_ENUMERATED_PATHS {
                return Err(CheckpointError::Budget("候选路径数量超出记录硬上限"));
            }
        }
    }
    if paths.len() > MAX_ENUMERATED_PATHS {
        return Err(CheckpointError::Budget("候选路径数量超出记录硬上限"));
    }
    let mut folded = BTreeMap::<Vec<u8>, &[u8]>::new();
    for path in paths.keys() {
        for end in path
            .iter()
            .enumerate()
            .filter_map(|(i, byte)| (*byte == b'/').then_some(i))
            .chain([path.len()])
        {
            let prefix = &path[..end];
            let normalized = match std::str::from_utf8(prefix) {
                Ok(s) => s.to_lowercase().into_bytes(),
                Err(_) => prefix.to_ascii_lowercase(),
            };
            if folded
                .insert(normalized, prefix)
                .is_some_and(|prior| prior != prefix)
            {
                return Err(CheckpointError::Invalid("大小写碰撞路径"));
            }
        }
    }
    let mut admitted = 0;
    Ok(paths
        .into_values()
        .map(|mut candidate| {
            if candidate.omission.is_none() {
                admitted += 1;
                if admitted > p.max_paths {
                    candidate.omission = Some(OmissionReason::PathBudget);
                }
            }
            candidate
        })
        .collect())
}

fn sensitive_path(path: &[u8]) -> bool {
    path.split(|b| *b == b'/').any(|p| {
        let p = p.to_ascii_lowercase();
        p == b".env"
            || p.starts_with(b".env.") && p != b".env.example"
            || matches!(
                p.as_slice(),
                b".ssh"
                    | b".aws"
                    | b".gnupg"
                    | b".azure"
                    | b".kube"
                    | b".config"
                    | b".claude"
                    | b".codex"
                    | b".pi"
                    | b".gemini"
                    | b".git-credentials"
                    | b".netrc"
                    | b"id_rsa"
                    | b"id_ed25519"
                    | b"credentials"
                    | b"redact.key"
                    | b"local-credential"
                    | b"daemon-keypair.json"
            )
            || p.ends_with(b".p12")
            || p.ends_with(b".pfx")
            || p.ends_with(b".key")
    })
}

fn sensitive_content(bytes: &[u8]) -> bool {
    let lower = bytes.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
    [
        b"private key-----".as_slice(),
        b"aws_secret_access_key",
        b"api_key=",
        b"api_key:",
        b"apikey=",
        b"access_token=",
        b"password=",
        b"authorization: bearer",
        b"ghp_",
        b"github_pat_",
        b"sk-proj-",
        b"akia",
    ]
    .iter()
    .any(|p| lower.windows(p.len()).any(|v| v == *p))
}

fn binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

pub(super) fn safe_path(path: &[u8]) -> Result<PathBuf, CheckpointError> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains(&0)
        || path.contains(&b'\\')
        || path.starts_with(b"/")
    {
        return Err(CheckpointError::Invalid("路径越界或歧义"));
    }
    let mut result = PathBuf::new();
    for segment in path.split(|b| *b == b'/') {
        if segment.is_empty()
            || segment == b"."
            || segment == b".."
            || segment.eq_ignore_ascii_case(b".git")
            || segment.contains(&b':')
            || segment.ends_with(b".")
            || segment.ends_with(b" ")
        {
            return Err(CheckpointError::Invalid("路径组件不安全"));
        }
        let stem = segment
            .split(|b| *b == b'.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if matches!(
            stem.as_slice(),
            b"CON" | b"PRN" | b"AUX" | b"NUL" | b"CONIN$" | b"CONOUT$"
        ) || stem.starts_with(b"GIT~")
            || ((stem.starts_with(b"COM") || stem.starts_with(b"LPT"))
                && ["¹", "²", "³"]
                    .iter()
                    .any(|digit| digit.as_bytes() == &stem[3..]))
            || stem.len() == 4
                && (stem.starts_with(b"COM") || stem.starts_with(b"LPT"))
                && matches!(stem[3], b'1'..=b'9')
        {
            return Err(CheckpointError::Invalid("Windows 保留路径"));
        }
        result.push(
            platform::native_component(segment)
                .ok_or(CheckpointError::Invalid("非 UTF-8 路径不受当前平台支持"))?,
        );
    }
    if !result
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(CheckpointError::Invalid("路径越界"));
    }
    Ok(result)
}

async fn ignored(git: &Git<'_>, path: &[u8], is_dir: bool) -> Result<bool, CheckpointError> {
    let mut input = path.to_vec();
    if is_dir {
        input.push(b'/');
    }
    input.push(0);
    Ok(!git
        .accepted(
            &strings(&["check-ignore", "--no-index", "-z", "--stdin"]),
            Some(&input),
            &[1],
        )
        .await?
        .is_empty())
}

async fn attributes_for(git: &Git<'_>, path: &[u8]) -> Result<Vec<u8>, CheckpointError> {
    let mut input = path.to_vec();
    input.push(0);
    git.run(
        &strings(&[
            "check-attr",
            "-z",
            "--stdin",
            "filter",
            "working-tree-encoding",
            "ident",
            "text",
            "eol",
        ]),
        Some(&input),
    )
    .await
}

fn transformation(attributes: &[u8]) -> Option<OmissionReason> {
    let parts = attributes.split(|b| *b == 0).collect::<Vec<_>>();
    for attr in parts.as_chunks::<3>().0 {
        if attr[2] == b"unspecified" || attr[2] == b"unset" {
            continue;
        }
        match attr[1] {
            b"filter" => return Some(OmissionReason::UnsupportedFilter),
            b"working-tree-encoding" | b"ident" => {
                return Some(OmissionReason::UnsupportedEncoding);
            }
            _ => {}
        }
    }
    None
}

async fn set_entry(
    git: &Git<'_>,
    mode: &str,
    oid: &str,
    path: &[u8],
) -> Result<(), CheckpointError> {
    let mut input = format!("{mode} {oid}\t").into_bytes();
    input.extend_from_slice(path);
    input.push(0);
    git.run(
        &strings(&["update-index", "-z", "--index-info"]),
        Some(&input),
    )
    .await?;
    Ok(())
}

async fn remove_entry(git: &Git<'_>, path: &[u8]) -> Result<(), CheckpointError> {
    let empty = git::oid(&git.text(&["write-tree"]).await?)?;
    set_entry(git, "0", &"0".repeat(empty.len()), path).await
}

fn omit(manifest: &mut CoverageManifest, candidate: &Candidate, reason: OmissionReason) {
    if reason != OmissionReason::Ignored {
        manifest.complete = false;
    }
    manifest.omissions.push(Omission {
        path: candidate.path.clone(),
        reason,
        retains_base_entry: candidate.base,
    });
}

#[allow(clippy::too_many_arguments)]
async fn verify_capture(
    git: &Git<'_>,
    root: &Directory,
    admin: &Directory,
    common: &Directory,
    before: &CheckpointObservation,
    candidates: &[Candidate],
    captured: &[Captured],
    attributes: &[(Vec<u8>, Vec<u8>)],
    parent: Option<&str>,
    policy: &CheckpointPolicy,
) -> Result<(), CheckpointError> {
    root.verify()?;
    admin.verify()?;
    common.verify()?;
    if observation(git, root).await? != *before
        || enumerate(git, root, admin, parent, policy).await? != candidates
    {
        return Err(CheckpointError::Drift);
    }
    validate_repository(git, admin).await?;
    for (path, attrs) in attributes {
        if attributes_for(git, path).await? != *attrs {
            return Err(CheckpointError::Drift);
        }
    }
    for file in captured {
        match (root.entry(&safe_path(&file.path)?)?, &file.oid) {
            (Entry::Absent, None) => {}
            (Entry::Regular(mut handle), Some(oid)) => {
                let (bytes, identity) = platform::read_bounded(
                    &mut handle,
                    policy.max_binary_bytes.max(policy.max_text_bytes),
                )?
                .ok_or(CheckpointError::Drift)?;
                if Some(identity) != file.identity || git.digest(&bytes).await? != *oid {
                    return Err(CheckpointError::Drift);
                }
            }
            _ => return Err(CheckpointError::Drift),
        }
    }
    Ok(())
}
