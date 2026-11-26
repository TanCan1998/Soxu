//! 已检出 Worktree 的宿主登记与 direct MCP 上下文准备。
//! 原生 Agent 启动与 Worktree 会话执行由后续合流接线。

use super::{DaemonState, internal_err, lock_err};
use anyhow::{Result, anyhow, ensure};
use soxu_git::{CheckoutStatus, GitProbe, WorktreeHead, WorktreeRegistrationCoordinator};
use soxu_magpie::{
    EnvironmentMode, WorktreeContext, WorktreeContextError, WorktreeDirectory,
    prepare_worktree_context,
};
use soxu_protocol::{ActionableError, ErrorCode, WorkspaceIsolation, WorkspaceSummary};
use std::path::Path;

/// 宿主授权的一次接入请求。路径和协调域由本机可信调用方指定。
pub struct WorktreeAdmissionRequest<'a> {
    pub state_root: &'a Path,
    pub request_id: &'a str,
    pub target: &'a Path,
    pub environment_mode: EnvironmentMode,
    pub magpie_config_dir: Option<&'a Path>,
}

/// Git 检出复核通过后的宿主登记结果。
#[derive(Debug)]
pub enum WorktreeAdmission {
    Ready {
        workspace: WorkspaceSummary,
        context: WorktreeContext,
    },
    ContextFailed {
        workspace: WorkspaceSummary,
        error: WorktreeContextError,
    },
}

impl DaemonState {
    /// 重验 Git 注册与检出后登记工作区。Git 结果与路由准备各自留痕。
    pub async fn admit_worktree(
        &self,
        git: &GitProbe,
        request: &WorktreeAdmissionRequest<'_>,
        workspace: WorkspaceSummary,
    ) -> Result<WorktreeAdmission> {
        let _guard = self.worktree_admission_lock.lock().await;
        let workspace_id = format!("worktree:{}", request.request_id);
        let old = self.store.get_workspace(&workspace_id)?;
        if let Some(old) = &old {
            ensure!(
                old.project_id == workspace.project_id,
                "工作区请求的项目已绑定"
            );
        }
        let workspace = old.clone().unwrap_or(workspace);
        self.admit_worktree_inner(git, request, workspace, workspace_id, old)
            .await
    }

    /// 从上次已登记的摘要重试上下文准备，每次都重新复核 Git。
    pub async fn retry_worktree_context(
        &self,
        git: &GitProbe,
        request: &WorktreeAdmissionRequest<'_>,
    ) -> Result<WorktreeAdmission> {
        let _guard = self.worktree_admission_lock.lock().await;
        let workspace_id = format!("worktree:{}", request.request_id);
        let old = self
            .store
            .get_workspace(&workspace_id)?
            .ok_or_else(|| anyhow!("工作区尚未经过宿主登记"))?;
        self.admit_worktree_inner(git, request, old.clone(), workspace_id, Some(old))
            .await
    }

    async fn admit_worktree_inner(
        &self,
        git: &GitProbe,
        request: &WorktreeAdmissionRequest<'_>,
        mut workspace: WorkspaceSummary,
        workspace_id: String,
        old: Option<WorkspaceSummary>,
    ) -> Result<WorktreeAdmission> {
        let target = request
            .target
            .to_str()
            .ok_or_else(|| anyhow!("工作区路径无法公开表示"))?;
        ensure!(!workspace.archived, "工作区已归档");
        if let Some(old) = &old {
            ensure!(
                matches!(old.isolation, WorkspaceIsolation::Worktree { .. })
                    && old.display_path == target,
                "工作区请求的目标已绑定"
            );
        }
        self.worktree_ready
            .lock()
            .map_err(|_| lock_err("worktree_ready"))?
            .remove(&workspace_id);

        let project = self
            .store
            .get_project(&workspace.project_id)?
            .ok_or_else(|| anyhow!("工作区项目尚未登记"))?;
        let verification = verify_worktree(git, request, Path::new(&project.root_path)).await;
        let (root, branch, base_ref) = match verification {
            Ok(verified) => verified,
            Err(error) => {
                if let Some(mut old) = old {
                    old.needs_recovery = true;
                    old.recovery_reason = Some("工作区 Git 注册或检出重验未通过".to_owned());
                    self.persist_workspace(old)?;
                }
                return Err(error);
            }
        };
        if let Some(old) = &old {
            ensure!(
                old.isolation
                    == (WorkspaceIsolation::Worktree {
                        branch: branch.clone(),
                        base_ref: base_ref.clone()
                    }),
                "工作区请求的分支或固定提交已绑定"
            );
        }
        workspace.workspace_id = workspace_id;
        workspace.project_name = project.name;
        workspace.display_path = target.to_owned();
        workspace.branch.clone_from(&branch);
        workspace.isolation = WorkspaceIsolation::Worktree { branch, base_ref };
        self.prepare_admitted_worktree(
            root,
            workspace,
            request.environment_mode.clone(),
            request.magpie_config_dir,
        )
    }

    fn prepare_admitted_worktree(
        &self,
        root: WorktreeDirectory,
        mut workspace: WorkspaceSummary,
        mode: EnvironmentMode,
        magpie_config_dir: Option<&Path>,
    ) -> Result<WorktreeAdmission> {
        let preparation = prepare_worktree_context(&root, mode, magpie_config_dir);
        workspace.needs_recovery = preparation.is_err();
        workspace.recovery_reason = preparation.as_ref().err().map(ToString::to_string);
        workspace.updated_at = chrono::Utc::now();
        self.persist_workspace(workspace.clone())?;
        match preparation {
            Ok(context) => {
                self.worktree_ready
                    .lock()
                    .map_err(|_| lock_err("worktree_ready"))?
                    .insert(workspace.workspace_id.clone(), root);
                Ok(WorktreeAdmission::Ready { workspace, context })
            }
            Err(error) => Ok(WorktreeAdmission::ContextFailed { workspace, error }),
        }
    }

    pub(super) fn require_workspace_for_session(
        &self,
        workspace_id: &str,
    ) -> Result<(), ActionableError> {
        let workspace = self
            .store
            .get_workspace(workspace_id)
            .map_err(internal_err)?
            .ok_or_else(|| {
                ActionableError::new(
                    ErrorCode::NotFound,
                    "指定的工作区不存在",
                    "无法创建该工作区下的会话",
                    "请先刷新工作区列表或重新绑定项目目录",
                )
            })?;
        if matches!(workspace.isolation, WorkspaceIsolation::Worktree { .. })
            && (workspace.archived
                || workspace.needs_recovery
                || !self.worktree_directory_ready(workspace_id)?)
        {
            return Err(ActionableError::new(
                ErrorCode::WorkspaceDamaged,
                "Worktree 尚未通过宿主接入复核",
                "无法创建该工作区下的会话",
                "请重新核验工作区并重试上下文准备",
            ));
        }
        Ok(())
    }

    pub(super) fn worktree_directory_ready(
        &self,
        workspace_id: &str,
    ) -> Result<bool, ActionableError> {
        Ok(self
            .worktree_ready
            .lock()
            .map_err(|_| lock_err("worktree_ready"))?
            .get(workspace_id)
            .is_some_and(|directory| directory.recheck().is_ok()))
    }
}

async fn verify_worktree(
    git: &GitProbe,
    request: &WorktreeAdmissionRequest<'_>,
    source: &Path,
) -> Result<(WorktreeDirectory, String, String)> {
    let coordinator = WorktreeRegistrationCoordinator::open(request.state_root, &[])?;
    let status = git.checkout_status(&coordinator, request.request_id)?;
    ensure!(
        status == CheckoutStatus::Verified,
        "Worktree 检出尚未通过核验（{status:?}）"
    );
    let target = WorktreeDirectory::open(request.target)?;
    let observed = git
        .diagnose_registration_with_target(
            request.state_root,
            request.request_id,
            source,
            request.target,
        )
        .await?
        .ok_or_else(|| anyhow!("持久注册请求不存在"))?;
    ensure!(
        observed.target_matches && observed.locked.is_none() && observed.prunable.is_none(),
        "Worktree 持久注册与当前 Git 身份不一致"
    );
    let identity = observed
        .target_identity
        .ok_or_else(|| anyhow!("缺少已核验目标身份"))?;
    ensure!(
        identity.root == target.path(),
        "Worktree 目录与 Git 身份不一致"
    );
    let entry = observed
        .inventory
        .entries
        .iter()
        .find(|entry| entry.path.to_path_buf().as_deref() == Ok(identity.root.as_path()))
        .ok_or_else(|| anyhow!("Git 清单缺少目标 Worktree"))?;
    let WorktreeHead::Attached { oid, branch } = &entry.head else {
        return Err(anyhow!("已登记 Worktree 缺少绑定分支"));
    };
    let branch = std::str::from_utf8(branch)?
        .strip_prefix("refs/heads/")
        .ok_or_else(|| anyhow!("Worktree 分支引用不合法"))?
        .to_owned();
    verify_checkout(git, request, &target, &identity.git_dir, oid).await?;
    let current = git
        .diagnose_registration_with_target(
            request.state_root,
            request.request_id,
            source,
            request.target,
        )
        .await?
        .ok_or_else(|| anyhow!("持久注册请求不存在"))?;
    ensure!(
        current.target_matches
            && current.target_identity.as_ref() == Some(&identity)
            && current.inventory == observed.inventory
            && current.ref_oid == observed.ref_oid
            && git.checkout_status(&coordinator, request.request_id)? == CheckoutStatus::Verified,
        "复核期间 Worktree 注册或检出状态发生变化"
    );
    target.recheck()?;
    Ok((target, branch, oid.clone()))
}

#[cfg(not(target_os = "linux"))]
async fn verify_checkout(
    _git: &GitProbe,
    _request: &WorktreeAdmissionRequest<'_>,
    _target: &WorktreeDirectory,
    _git_dir: &Path,
    _oid: &str,
) -> Result<()> {
    Err(soxu_git::WorktreeLifecycleError::UnsupportedFilesystem(
        "缺少受控检出复核所需的目录句柄路径",
    )
    .into())
}

#[cfg(target_os = "linux")]
async fn verify_checkout(
    git: &GitProbe,
    request: &WorktreeAdmissionRequest<'_>,
    target: &WorktreeDirectory,
    git_dir: &Path,
    oid: &str,
) -> Result<()> {
    // #51 的私有暂存保留了已验证的 index 与 checkout-index 产物。
    // 字节比较保留 text/eol 等检出语义，同时避免在目标仓库中执行 Git 转换。
    let stage = request
        .state_root
        .join(format!("checkout-{}.stage", request.request_id));
    let admin = WorktreeDirectory::open(&stage.join("admin"))?;
    let staged = WorktreeDirectory::open(&stage.join("work"))?;
    let live_admin = WorktreeDirectory::open(git_dir)?;
    compare_file(&admin, &live_admin, Path::new("index"), false)?;

    let tree = read_checkout_git(git, &admin, &["ls-tree", "-r", "-z", "--full-tree", oid]).await?;
    let index = read_checkout_git(git, &admin, &["ls-files", "--stage", "-z"]).await?;
    ensure!(
        tree.is_empty() || tree.ends_with(b"\0"),
        "固定提交树的条目格式无效"
    );
    let mut expected = Vec::new();
    for entry in tree.split_inclusive(|byte| *byte == 0) {
        let entry = std::str::from_utf8(&entry[..entry.len() - 1])?;
        let (info, path) = entry
            .split_once('\t')
            .ok_or_else(|| anyhow!("固定提交树条目缺少路径"))?;
        let (mode, oid) = info
            .split_once(" blob ")
            .ok_or_else(|| anyhow!("固定提交树含不支持的条目"))?;
        ensure!(
            matches!(mode, "100644" | "100755"),
            "固定提交树文件模式不受支持"
        );
        let relative = Path::new(path);
        ensure!(
            !path.is_empty()
                && relative
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_))),
            "固定提交树文件路径不受支持"
        );
        expected.extend_from_slice(format!("{mode} {oid} 0	{path}\0").as_bytes());
        compare_file(&staged, target, relative, mode == "100755")?;
    }
    ensure!(index == expected, "Worktree index 与固定提交树不一致");
    compare_file(&admin, &live_admin, Path::new("index"), false)?;
    admin.recheck()?;
    staged.recheck()?;
    live_admin.recheck()?;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn read_checkout_git(
    git: &GitProbe,
    admin: &WorktreeDirectory,
    args: &[&str],
) -> Result<Vec<u8>> {
    let anchor = admin.anchored_path();
    let fd = anchor
        .file_name()
        .ok_or_else(|| anyhow!("无法定位检出暂存目录句柄"))?;
    let child_anchor =
        std::path::PathBuf::from(format!("/proc/{}/fd", std::process::id())).join(fd);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(git.program())
            .current_dir("/")
            .env_clear()
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_ALLOW_PROTOCOL", "")
            .env("GIT_DIR", child_anchor)
            .env("GIT_WORK_TREE", "/")
            .kill_on_drop(true)
            .args(args)
            .output(),
    )
    .await??;
    ensure!(
        output.status.success() && output.stderr.is_empty(),
        "Git 无法只读复核检出暂存"
    );
    Ok(output.stdout)
}

#[cfg(target_os = "linux")]
fn compare_file(
    first: &WorktreeDirectory,
    second: &WorktreeDirectory,
    relative: &Path,
    executable: bool,
) -> Result<()> {
    use std::fs::{File, Metadata, OpenOptions};
    use std::io::{BufRead, BufReader};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    fn open_file(
        root: &WorktreeDirectory,
        relative: &Path,
    ) -> Result<(File, WorktreeDirectory, Metadata)> {
        let parent = WorktreeDirectory::open(
            &root
                .path()
                .join(relative.parent().expect("相对文件路径有父目录")),
        )?;
        let name = relative
            .file_name()
            .ok_or_else(|| anyhow!("检出文件路径无效"))?;
        let path = parent.anchored_path().join(name);
        let before = path.symlink_metadata()?;
        ensure!(before.is_file(), "检出目标含非普通文件");
        // Linux O_NONBLOCK 避免被替换成 FIFO 时阻塞，后续通过打开的文件句柄确认身份。
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(0o4000)
            .open(&path)?;
        let opened = file.metadata()?;
        ensure!(same_file(&before, &opened), "检出文件身份发生变化");
        root.recheck()?;
        Ok((file, parent, opened))
    }

    fn same_file(a: &Metadata, b: &Metadata) -> bool {
        a.is_file()
            && b.is_file()
            && a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.len() == b.len()
            && a.ctime() == b.ctime()
            && a.ctime_nsec() == b.ctime_nsec()
    }

    let (a, a_parent, a_meta) = open_file(first, relative)?;
    let (b, b_parent, b_meta) = open_file(second, relative)?;
    ensure!(
        a_meta.len() == b_meta.len() && (b_meta.mode() & 0o100 != 0) == executable,
        "Worktree 检出大小或文件模式已变化"
    );
    let mut a = BufReader::new(a);
    let mut b = BufReader::new(b);
    loop {
        let left = a.fill_buf()?;
        let right = b.fill_buf()?;
        let n = left.len().min(right.len());
        ensure!(
            left[..n] == right[..n] && (left.is_empty() == right.is_empty()),
            "Worktree 检出内容已变化"
        );
        if n == 0 {
            break;
        }
        a.consume(n);
        b.consume(n);
    }
    let name = relative.file_name().expect("已验证文件名");
    ensure!(
        same_file(
            &a_meta,
            &a_parent.anchored_path().join(name).symlink_metadata()?
        ) && same_file(
            &b_meta,
            &b_parent.anchored_path().join(name).symlink_metadata()?
        ),
        "复核期间检出文件身份发生变化"
    );
    a_parent.recheck()?;
    b_parent.recheck()?;
    first.recheck()?;
    second.recheck()?;
    Ok(())
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;
    use soxu_magpie::WORKTREE_CONTEXT_FILE;
    use soxu_protocol::WorkspaceBucket;
    use std::fs;

    #[test]
    fn verified_directory_fixture_only_becomes_ready_after_context_preparation() {
        let fixture = tempfile::tempdir().expect("夹具操作应成功");
        let target = fixture.path().join("worktree");
        let magpie = fixture.path().join("magpie");
        fs::create_dir(&target).expect("夹具操作应成功");
        fs::create_dir(&magpie).expect("夹具操作应成功");
        fs::write(target.join(".git"), b"gitdir: fixture").expect("夹具操作应成功");
        fs::write(
            magpie.join("library.json"),
            br#"{"mcpHub":false,"mcp":[{"name":"aoci"}]}"#,
        )
        .expect("夹具操作应成功");
        let manifest = target.join(WORKTREE_CONTEXT_FILE);
        fs::write(&manifest, b"user file").expect("夹具操作应成功");
        let workspace_id = "worktree:fixture";
        let state = DaemonState::open_with_environment(
            None,
            "http://localhost",
            false,
            EnvironmentMode::MagpieManaged,
        )
        .expect("夹具操作应成功");
        let workspace = WorkspaceSummary {
            workspace_id: workspace_id.to_owned(),
            project_id: "fixture".to_owned(),
            project_name: "测试项目".to_owned(),
            display_path: target.to_str().expect("夹具操作应成功").to_owned(),
            branch: "test".to_owned(),
            isolation: WorkspaceIsolation::Worktree {
                branch: "test".to_owned(),
                base_ref: "base".to_owned(),
            },
            agent_id: "codex".to_owned(),
            model_id: String::new(),
            bucket: WorkspaceBucket::Done,
            summary: String::new(),
            diff_stat: String::new(),
            label: None,
            archived: false,
            needs_recovery: false,
            recovery_reason: None,
            updated_at: chrono::Utc::now(),
        };
        let WorktreeAdmission::ContextFailed {
            workspace,
            error: WorktreeContextError::Conflict,
        } = state
            .prepare_admitted_worktree(
                WorktreeDirectory::open(&target).expect("夹具操作应成功"),
                workspace,
                EnvironmentMode::MagpieManaged,
                Some(&magpie),
            )
            .expect("夹具操作应成功")
        else {
            panic!("路由失败应保留可观察的工作区摘要");
        };
        assert!(workspace.needs_recovery);
        assert_eq!(
            state
                .require_workspace_for_session(workspace_id)
                .expect_err("工作区会话应被拒绝")
                .code,
            ErrorCode::WorkspaceDamaged
        );
        assert_eq!(fs::read(&manifest).expect("夹具操作应成功"), b"user file");

        fs::remove_file(&manifest).expect("夹具操作应成功");
        let root = WorktreeDirectory::open(&target).expect("夹具操作应成功");
        assert!(matches!(
            state
                .prepare_admitted_worktree(
                    root,
                    workspace,
                    EnvironmentMode::MagpieManaged,
                    Some(&magpie)
                )
                .expect("夹具操作应成功"),
            WorktreeAdmission::Ready {
                context: WorktreeContext::Prepared { .. },
                ..
            }
        ));
        assert_eq!(
            state.snapshot().expect("夹具操作应成功").workspaces.len(),
            1
        );
        assert!(state.require_workspace_for_session(workspace_id).is_ok());
    }
}
