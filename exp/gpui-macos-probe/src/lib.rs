//! Soxu #93 隔离实验夹具：GPUI 最小可交互实体。
//!
//! 本 crate 只用于在真实 GitHub 托管 Runner 上验证 GPUI 的测试能力边界，
//! 不进入 Soxu 产品 workspace，也不被任何产品 Crate 依赖。

use std::ops::Range;

use gpui::{
    App, Bounds, Context, EntityInputHandler, FocusHandle, Focusable, Pixels, Render, UTF16Selection,
    Window, actions, div, prelude::*, px,
};

actions!(probe, [Bump]);

/// UTF-16 长度换算，供 IME 标记区与光标选区使用。
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// 把 UTF-16 偏移映射回字节偏移，越界时收敛到最近边界。
pub fn byte_index_for_utf16(text: &str, target: usize) -> usize {
    let mut utf16 = 0usize;
    for (byte, ch) in text.char_indices() {
        if utf16 >= target {
            return byte;
        }
        utf16 += ch.len_utf16();
    }
    text.len()
}

/// 最小可交互实体：持有文本、标记区、选区与焦点，并实现 IME 输入处理器。
pub struct ProbeEditor {
    text: String,
    marked: Option<Range<usize>>,
    selected: Range<usize>,
    focus_handle: FocusHandle,
    bumps: usize,
}

impl ProbeEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            text: String::new(),
            marked: None,
            selected: 0..0,
            focus_handle: cx.focus_handle(),
            bumps: 0,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn bumps(&self) -> usize {
        self.bumps
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.text.clear();
        self.text.push_str(text);
        self.marked = None;
        self.selected = 0..0;
        cx.notify();
    }

    fn replace(&mut self, range: Option<Range<usize>>, new_text: &str) -> Range<usize> {
        let range = range.unwrap_or_else(|| self.selected.clone());
        let start = byte_index_for_utf16(&self.text, range.start);
        let end = byte_index_for_utf16(&self.text, range.end);
        self.text.replace_range(start..end, new_text);
        let caret = utf16_len(&self.text[..start + new_text.len()]);
        self.selected = caret..caret;
        caret..caret
    }

    fn bump(&mut self, _: &Bump, _window: &mut Window, cx: &mut Context<Self>) {
        self.bumps += 1;
        cx.notify();
    }
}

impl Focusable for ProbeEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for ProbeEditor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let start = byte_index_for_utf16(&self.text, range.start);
        let end = byte_index_for_utf16(&self.text, range.end);
        *adjusted_range = Some(utf16_len(&self.text[..start])..utf16_len(&self.text[..end]));
        Some(self.text[start..end].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.selected.clone(),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked.clone()
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range, text);
        self.marked = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let caret = self.replace(range, new_text);
        let marked_len = utf16_len(new_text);
        self.marked = Some(caret.start..caret.start + marked_len);
        if let Some(selected) = new_selected_range {
            self.selected = caret.start + selected.start..caret.start + selected.end;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Render for ProbeEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("probe-root")
            .track_focus(&self.focus_handle)
            .key_context("ProbeEditor")
            .on_action(cx.listener(Self::bump))
            .w(px(320.))
            .h(px(120.))
            .bg(gpui::rgb(0x000000))
            .p(px(12.))
            .child(
                div()
                    .id("probe-label")
                    .debug_selector(|| "probe-label".to_string())
                    .w(px(200.))
                    .h(px(24.))
                    .child(self.text.clone()),
            )
    }
}
