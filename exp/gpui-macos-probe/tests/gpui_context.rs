//! GPUI 测试能力边界探针。
//!
//! 四条用例分别覆盖：平台无关逻辑、隔离 `TestAppContext` 的实体交互、
//! `VisualTestContext` 的窗口焦点/动作/布局，以及 IME 标记区语义。
//! 任何一条失败都必须原样记录，不得改写为通过。

use std::time::Duration;

use gpui::{
    AppContext, Bounds, Entity, EntityInputHandler, Focusable, Modifiers, TestAppContext,
    VisualTestContext, px,
};
use gpui_macos_probe::{Bump, ProbeEditor, byte_index_for_utf16, utf16_len};

fn open_editor(cx: &mut TestAppContext) -> (Entity<ProbeEditor>, &mut VisualTestContext) {
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = ProbeEditor::new(cx);
        editor.focus_handle(cx).focus(window);
        editor
    });
    cx.run_until_parked();
    (editor, cx)
}

/// 第一层：不接触任何 GPUI 上下文与窗口的纯逻辑。
#[test]
fn utf16_offsets_round_trip_for_chinese_text() {
    let text = "配置 Codex 驱动";
    assert_eq!(utf16_len(text), 11);
    assert_eq!(byte_index_for_utf16(text, 0), 0);
    assert_eq!(byte_index_for_utf16(text, 2), 6);
    assert_eq!(byte_index_for_utf16(text, 11), text.len());
}

/// 第二层：只有 `TestAppContext`，没有窗口渲染。
#[gpui::test]
fn entity_update_emits_without_a_window(cx: &mut TestAppContext) {
    // 0.2.2 的 `TestAppContext::new` 是私有继承方法，只能经 `App::new` 建实体。
    let editor = cx.update(|cx| cx.new(ProbeEditor::new));
    editor.update(cx, |editor, cx| {
        editor.set_text("流式 Transcript", cx);
    });
    let text = editor.read_with(cx, |editor, _| editor.text().to_string());
    assert_eq!(text, "流式 Transcript");
}

/// 第三层：真实窗口、真实焦点与真实动作派发。
#[gpui::test]
fn focus_and_action_dispatch_reach_the_render_tree(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);

    let focused = cx.update(|window, cx| {
        let expected = editor.read(cx).focus_handle(cx);
        window.focused(cx).as_ref() == Some(&expected)
    });
    assert!(focused, "实体焦点必须挂到窗口上");

    cx.dispatch_action(Bump);
    let bumps = editor.read_with(cx, |editor, _| editor.bumps());
    assert_eq!(bumps, 1, "动作必须经渲染树解析到被聚焦实体");
}

/// 第三层：固定宽度下的布局测量，用选择器取边界而不是比像素图。
#[gpui::test]
fn layout_bounds_are_queryable_by_debug_selector(cx: &mut TestAppContext) {
    let (_editor, cx) = open_editor(cx);
    let bounds = cx
        .debug_bounds("probe-label")
        .expect("固定选择器必须能被 debug_bounds 命中");
    let _: Bounds<gpui::Pixels> = bounds;
    assert_eq!(bounds.size.width, px(200.), "标签宽度必须等于声明宽度");
    assert!(bounds.size.height > px(0.), "标签必须完成一次真实布局");
}

/// 第三层：中文长文本的 IME 标记区直接经 `EntityInputHandler` 驱动。
#[gpui::test]
fn ime_marked_range_keeps_utf16_caret(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.replace_text_in_range(None, "ab", window, cx);
            editor.replace_and_mark_text_in_range(None, "中文", Some(0..2), window, cx);
        });
    });

    let (text, marked) = editor.read_with(cx, |editor, _| (editor.text().to_string(), editor.marked()));
    assert_eq!(text, "ab中文");
    assert_eq!(marked, Some(4..6), "标记区必须按 UTF-16 计数");
}

/// 第三层：鼠标点击命中与窗口缩放都只影响布局状态。
#[gpui::test]
fn click_and_resize_do_not_panic(cx: &mut TestAppContext) {
    let (_editor, cx) = open_editor(cx);
    cx.simulate_mouse_move(gpui::point(px(10.), px(10.)), None, Modifiers::default());
    cx.simulate_click(gpui::point(px(10.), px(10.)), Modifiers::default());
    cx.simulate_resize(gpui::size(px(640.), px(480.)));
    cx.run_until_parked();
}

/// 异步：测试执行器在无外部等待时只跑到 parked 为止。
#[gpui::test]
async fn detached_tasks_run_only_when_parked(cx: &mut TestAppContext) {
    let editor = cx.update(|cx| cx.new(ProbeEditor::new));
    let task = cx.spawn({
        let editor = editor.clone();
        async move |cx| {
            cx.update(|cx| editor.update(cx, |editor, cx| editor.set_text("后台任务", cx)))
                .ok();
        }
    });
    task.detach();
    cx.run_until_parked();
    let text = editor.read_with(cx, |editor, _| editor.text().to_string());
    assert_eq!(text, "后台任务");
}

/// 异步：跨线程等待必须显式放行，否则 GPUI 测试执行器按设计 panic。
#[gpui::test]
async fn external_thread_requires_allow_parking(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (tx, rx) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5));
        tx.send(42).ok();
    });
    assert_eq!(rx.await.unwrap(), 42);
}
