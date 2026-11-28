//! 真实 PTY 后端：Windows ConPTY / Unix `openpty`（经 `portable-pty`）。
//!
//! 本工作区 `unsafe_code = "forbid"`，无法在本 crate 内直接封装 ConPTY 与 `openpty`，
//! 因此把系统调用面收敛到一个受审计的跨平台 crate；本模块只做参数换算、
//! 环境白名单注入与错误模型映射。
//!
//! 与脚本域的唯一区别：**终端必须派生交互 shell**（脚本域则严禁解释器）。
//! 该 shell 由 `SOXU_PTY_SHELL` 或平台默认值确定，属受控来源，不接受客户端指定。

use crate::pty::{PtyBackend, PtyChild, PtyControl, PtySpawnSpec};
use crate::scripts::manifest::child_facing_path;
use portable_pty::{
    Child, CommandBuilder, ExitStatus as PtyProcessStatus, MasterPty, PtyPair,
    PtySize as RawPtySize, native_pty_system,
};
use soxu_protocol::{ActionableError, ErrorCode, PtyExitStatus, PtySize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// 受控 shell 的环境变量名（仅接受绝对路径的已存在文件）。
pub const PTY_SHELL_ENV: &str = "SOXU_PTY_SHELL";

/// 真实 PTY 后端。
#[derive(Debug, Clone)]
pub struct PortablePtyBackend {
    shell: PathBuf,
}

impl PortablePtyBackend {
    /// 按 `SOXU_PTY_SHELL` 或平台默认值确定受控 shell。
    pub fn discover() -> Result<Self, ActionableError> {
        Self::with_shell(resolve_shell()?)
    }

    /// 以显式 shell 构造（测试与宿主装配用）。
    pub fn with_shell(shell: PathBuf) -> Result<Self, ActionableError> {
        if !shell.is_absolute() {
            return Err(shell_error("终端 shell 必须是绝对路径"));
        }
        if !shell.is_file() {
            return Err(shell_error("终端 shell 不是已存在的文件"));
        }
        Ok(Self { shell })
    }

    /// 受控 shell 路径。
    #[must_use]
    pub fn shell(&self) -> &Path {
        &self.shell
    }
}

impl PtyBackend for PortablePtyBackend {
    fn spawn(&self, spec: &PtySpawnSpec) -> Result<PtyChild, ActionableError> {
        let pair = native_pty_system()
            .openpty(raw_size(spec.size))
            .map_err(|err| spawn_error("无法打开伪终端", err.to_string()))?;
        let PtyPair { master, slave } = pair;
        let mut builder = CommandBuilder::new(&self.shell);
        builder.cwd(child_facing_path(&spec.workdir).as_os_str());
        builder.env_clear();
        for (key, value) in platform_environment(&spec.environment) {
            builder.env(key, value);
        }
        let child = slave
            .spawn_command(builder)
            .map_err(|err| spawn_error("无法在伪终端内派生 shell", err.to_string()))?;
        drop(slave);
        let reader = master
            .try_clone_reader()
            .map_err(|err| spawn_error("无法获取终端读端", err.to_string()))?;
        let writer = master
            .take_writer()
            .map_err(|err| spawn_error("无法获取终端写端", err.to_string()))?;
        let control: Arc<dyn PtyControl> = Arc::new(PortablePtyControl {
            master: Mutex::new(master),
            child: Mutex::new(child),
        });
        Ok(PtyChild {
            reader,
            writer,
            control,
        })
    }
}

struct PortablePtyControl {
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
}

impl PtyControl for PortablePtyControl {
    fn resize(&self, size: PtySize) -> Result<(), ActionableError> {
        lock(&self.master).resize(raw_size(size)).map_err(|err| {
            ActionableError::new(
                ErrorCode::Conflict,
                "终端尺寸调整失败",
                "本次尺寸未生效，会话仍按原尺寸运行",
                "请重试或重新打开终端会话",
            )
            .with_diagnostic(err.to_string())
        })
    }

    fn terminate(&self) -> Result<(), ActionableError> {
        let mut child = lock(&self.child);
        if let Err(err) = child.kill() {
            // 已经自然退出的会话视为终止成功，避免把幂等关闭变成错误。
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                return Ok(());
            }
            return Err(ActionableError::new(
                ErrorCode::Conflict,
                "终端 shell 终止失败",
                "会话标记为失败，输出流已结束",
                "请重新打开终端会话",
            )
            .with_diagnostic(err.to_string()));
        }
        Ok(())
    }

    fn try_wait(&self) -> Result<Option<PtyExitStatus>, ActionableError> {
        let mut child = lock(&self.child);
        child
            .try_wait()
            .map(|status| status.map(map_exit))
            .map_err(|err| {
                ActionableError::new(
                    ErrorCode::InternalError,
                    "无法查询终端 shell 退出状态",
                    "会话状态可能滞后，不会影响输出分片序号",
                    "请重新查询会话快照",
                )
                .with_diagnostic(err.to_string())
            })
    }
}

/// 在客户端白名单之外补齐**机器级、无凭据**的基线变量。
///
/// ConPTY 的 `CreateProcessW` 要求环境块是「以 NUL 结尾的字符串块 + 结尾 NUL」；
/// 白名单为空时整块退化为单个 NUL，会被 Windows 判为参数错误（`os error 87`）。
/// 因此 Windows 上固定注入 `SystemRoot`/`WINDIR`（机器级、不涉用户凭据）作为最小基线，
/// 除此之外不会注入任何用户环境。Unix `openpty` 对空环境块没有这个约束，保持原样。
#[cfg(windows)]
fn platform_environment(allowlisted: &[(String, String)]) -> Vec<(String, String)> {
    let mut environment = allowlisted.to_vec();
    for key in ["SystemRoot", "WINDIR"] {
        let present = environment
            .iter()
            .any(|(existing, _)| existing.eq_ignore_ascii_case(key));
        if present {
            continue;
        }
        if let Ok(value) = std::env::var(key) {
            environment.push((key.to_owned(), value));
        }
    }
    environment
}

#[cfg(not(windows))]
fn platform_environment(allowlisted: &[(String, String)]) -> Vec<(String, String)> {
    allowlisted.to_vec()
}

fn raw_size(size: PtySize) -> RawPtySize {
    RawPtySize {
        rows: size.rows,
        cols: size.columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn map_exit(status: PtyProcessStatus) -> PtyExitStatus {
    PtyExitStatus {
        exit_code: i32::try_from(status.exit_code()).ok(),
        exit_signal: status.signal().and_then(parse_signal),
    }
}

/// 信号名只在与协议 `i32` 字段可无损对应时回填，其余保持 `None`（不臆造信号号）。
fn parse_signal(raw: &str) -> Option<i32> {
    raw.trim()
        .strip_prefix("SIG")
        .and_then(|name| name.parse::<i32>().ok())
        .or_else(|| raw.trim().parse::<i32>().ok())
}

fn resolve_shell() -> Result<PathBuf, ActionableError> {
    if let Some(raw) = std::env::var_os(PTY_SHELL_ENV) {
        let path = PathBuf::from(raw);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
        return Err(shell_error(&format!(
            "{PTY_SHELL_ENV} 必须指向一个已存在的绝对路径"
        )));
    }
    #[cfg(windows)]
    {
        // 先取系统自带的稳定绝对路径：WindowsApps 包的 pwsh 别名目录
        // 在 CreateProcessW 下会以引号包裹的应用名启动失败（os error 87），
        // 属包别名层限制，不能当作可用受控 shell。
        if let Some(root) = std::env::var_os("SystemRoot").map(PathBuf::from) {
            let fallback = root
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe");
            if fallback.is_file() {
                return Ok(fallback);
            }
        }
        if let Some(path) = find_in_path("pwsh.exe").or_else(|| find_in_path("powershell.exe")) {
            return Ok(path);
        }
        Err(shell_error(
            "未找到可用的 PowerShell，可通过 SOXU_PTY_SHELL 指定",
        ))
    }
    #[cfg(unix)]
    {
        if let Some(raw) = std::env::var_os("SHELL") {
            let path = PathBuf::from(raw);
            if path.is_absolute() && path.is_file() {
                return Ok(path);
            }
        }
        let fallback = PathBuf::from("/bin/sh");
        if fallback.is_file() {
            return Ok(fallback);
        }
        Err(shell_error(
            "未找到可用的 shell，可通过 SOXU_PTY_SHELL 指定",
        ))
    }
    #[cfg(not(any(windows, unix)))]
    {
        Err(shell_error("当前平台不支持受控终端"))
    }
}

#[cfg(windows)]
fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .filter(|dir| {
            // 跳过应用包别名目录（`...\WindowsApps\...\`），其可执行仅能经包激活启动。
            !dir.components()
                .any(|component| component.as_os_str().eq_ignore_ascii_case("WindowsApps"))
        })
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}

fn shell_error(summary: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::DriverUnavailable,
        summary,
        "受控终端本次不可用，其他能力不受影响",
        "请安装受支持的 shell，或用 SOXU_PTY_SHELL 指向绝对路径的 shell 程序",
    )
}

fn spawn_error(summary: &str, detail: String) -> ActionableError {
    ActionableError::new(
        ErrorCode::InternalError,
        summary,
        "该请求未打开终端会话，未派生任何进程",
        "请重试；若持续失败请检查 shell 与工作目录权限",
    )
    .with_diagnostic(detail)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_shell_must_be_absolute_existing_file() {
        assert!(PortablePtyBackend::with_shell(PathBuf::from("pwsh")).is_err());
        assert!(PortablePtyBackend::with_shell(PathBuf::from("/definitely/not/here")).is_err());
    }

    #[test]
    fn discovery_yields_an_existing_shell_or_a_clear_error() {
        match PortablePtyBackend::discover() {
            Ok(backend) => assert!(backend.shell().is_file(), "受控 shell 必须真实存在"),
            Err(err) => {
                assert_eq!(err.code, ErrorCode::DriverUnavailable);
                assert!(!err.remediation_zh.is_empty());
            }
        }
    }

    #[test]
    fn signal_names_map_only_when_numeric() {
        assert_eq!(parse_signal("SIG9"), Some(9));
        assert_eq!(parse_signal("9"), Some(9));
        assert_eq!(parse_signal("Killed"), None);
    }
}
