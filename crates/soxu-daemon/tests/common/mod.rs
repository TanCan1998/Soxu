//! `soxu-daemon` 集成测试共用的夹具辅助。

use std::path::PathBuf;

use tempfile::TempDir;

/// 返回测试自建夹具的根路径，供可信资产、启动目录与每次重验共用同一根。
///
/// 可证明事实：
/// - `NativeAssetGuard` 会拒绝任何含符号链接的祖先目录，未规范化的根会在
///   `checked_directory` 处被正常拒绝，负向断言因此到不了被测分支；
/// - 在 Linux 上用受控 symlink 作 `TMPDIR` 可复现与 CI 同形的失败
///   （`cargo test -p soxu-daemon --test native_assets` 1 通过 / 5 失败，退出码 101，
///   失败原因均为「目录或祖先不是普通目录」），规范化后同一条件 6/6 通过；
/// - Windows 上 `canonicalize` 会附加 `\\?\` 扩展路径前缀，实测会破坏
///   `cmd mklink /J` 与 Node 夹具的 cwd 契约，故保留 `TempDir` 既有路径。
///
/// Darwin Runner 上系统临时目录的具体别名形态尚无直接留证（远端日志只记录了拒绝
/// 原因），本条只做「夹具根须取自真实路径」的可泛化结论。
///
/// 只规范化测试自己创建的临时夹具；有意构造的链接、junction、恶意路径与被校验的
/// 资产或子路径一律原样交给守卫。
pub(crate) fn fixture_root(dir: &TempDir) -> PathBuf {
    let root = dir.path().to_path_buf();
    #[cfg(unix)]
    let root = root.canonicalize().expect("规范化隔离夹具根");
    root
}
