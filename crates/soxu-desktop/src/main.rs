//! Soxu GPUI 桌面工作台入口（M4-W2B 壳层底座）。
//!
//! 职责边界（任务书 Soxu#124）：
//! - 挂载 GPUI App 与主窗口：标题/尺寸/最小尺寸按 `design-system/soxu/MASTER.md`；
//! - 发布全局主题状态（`ThemeState`），支持明暗切换与跟随系统；
//! - 进程退出码与日志行为保持 `soxu doctor` 等现有用例不受影响。
//!
//! 业务视图（对话流、输入舱、审阅栏等）在 M5 落地；本阶段窗口内容为
//! [`soxu_desktop::shell::WorkbenchShell`] 四栏自适应骨架 + 占位面板。

use gpui::{
    App, Bounds, Context, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, point, prelude::*, px, size,
};
use soxu_protocol::PROTOCOL_VERSION;

use soxu_desktop::shell::WorkbenchShell;
use soxu_desktop::theme::theme_state::{ThemeMode, ThemeState};

/// 主窗口默认逻辑尺寸（宽 × 高，px）。
///
/// M4 阶段按四栏工作台的最小舒适形态取值；待 M5 视图落地后再按
/// `pages/01-desktop-workbench.md` 的持久化窗口布局需求校准。
const DEFAULT_WINDOW_WIDTH: f32 = 1120.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 720.0;

/// 主窗口最小逻辑尺寸：低于 400px 宽将触发超紧凑断点（`layout.breakpointUltraCompact`），
/// 允许压到 `ultra_compact` 可用下限；高度保底避免状态栏与输入舱重叠。
const MIN_WINDOW_WIDTH: f32 = 360.0;
const MIN_WINDOW_HEIGHT: f32 = 480.0;

/// 主窗口标题：`Soxu` 为专有品牌名，不作翻译且不加任何状态徽标（MASTER 品牌纪律）。
const WINDOW_TITLE: &str = "Soxu";

fn main() {
    // 日志行为与 CLI 侧一致：env-filter 由 `SOXU_LOG`/RUST_LOG 控制，缺省 info。
    // 初始化失败不 panic——桌面壳层的日志缺失不应破坏 doctor 等用例的退出码语义。
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        protocol = PROTOCOL_VERSION,
        "Soxu 桌面工作台启动"
    );

    gpui_platform::application().run(|cx: &mut App| {
        // 初始主题：跟随系统（读系统 app appearance）。默认深色是
        // `tokens.json` 的 defaultMode，但跟随系统优先——与设置页「外观与主题」一致。
        cx.set_global(ThemeState::with_preference(
            ThemeMode::System,
            cx.window_appearance(),
        ));

        let bounds = Bounds::centered(
            None,
            size(px(DEFAULT_WINDOW_WIDTH), px(DEFAULT_WINDOW_HEIGHT)),
            cx,
        );
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(WINDOW_TITLE.into()),
                    // Windows 上透明标题栏以延续自绘 48px Titlebar（MASTER sizing.titlebarHeight）。
                    appears_transparent: cfg!(any(target_os = "windows", target_os = "macos")),
                    traffic_light_position: cfg!(target_os = "macos")
                        .then(|| point(px(16.0), px(17.0))),
                }),
                window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            |window: &mut Window, cx: &mut App| {
                // 系统明暗变化时同步全局主题状态（跟随系统模式）。
                // `observe_window_appearance` 返回的 Subscription 随窗口存活，无需持有。
                let _subscription = window.observe_window_appearance(|window, cx| {
                    let appearance = window.appearance();
                    cx.update_global::<ThemeState, _>(|state, _| {
                        state.set_system_appearance(appearance);
                    });
                });
                cx.new(|cx: &mut Context<WorkbenchShell>| WorkbenchShell::new(cx))
            },
        )
        .expect("Soxu 主窗口创建失败");

        cx.activate(true);
    });
}

/// 供 `FocusLayer` 枚举与窗口几何常量的编译期完整性检查。
const _: () = {
    assert!(
        soxu_desktop::focus::FocusLayer::COUNT == 7,
        "焦点栈必须保持 7 层"
    );
    // 窗口几何常量与设计令牌断点的一致性（编译期守卫）：
    // 最小宽必须落入 `ultra_compact` 断点以下且不低于规范最窄 320px 视口；
    // 默认宽必须达到 `wide` 断点以呈现四栏；高度必须容纳 Titlebar + Statusbar。
    assert!(MIN_WINDOW_WIDTH < soxu_desktop::tokens::layout::BREAKPOINT_ULTRA_COMPACT_PX);
    assert!(
        MIN_WINDOW_WIDTH >= 320.0,
        "不得低于规范支持的最窄 320px 视口"
    );
    assert!(DEFAULT_WINDOW_WIDTH >= soxu_desktop::tokens::layout::BREAKPOINT_WIDE_PX);
    assert!(
        DEFAULT_WINDOW_HEIGHT
            > soxu_desktop::tokens::sizing::TITLEBAR_HEIGHT_PX
                + soxu_desktop::tokens::sizing::STATUSBAR_HEIGHT_PX
    );
};
