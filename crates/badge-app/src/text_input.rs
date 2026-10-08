//! Single-line text input with optional per-character colors (adapted
//! from GPUI's `input` example).

use std::ops::Range;

use badge_core::LedColor;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection,
    UnderlineStyle, Window, actions, div, fill, point, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy
    ]
);

pub enum InputEvent {
    Changed,
}

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    scroll_x: Pixels,
    /// Per-character colors (`None` = plain text input).
    colors: Option<Vec<LedColor>>,
    current_color: LedColor,
    mono_tint: LedColor,
    /// Where the right-click menu is open, in window coordinates.
    context_menu: Option<Point<Pixels>>,
}

impl EventEmitter<InputEvent> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        TextInput {
            focus_handle: cx.focus_handle().tab_stop(true),
            content: SharedString::default(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            scroll_x: px(0.),
            colors: None,
            current_color: LedColor::RED,
            mono_tint: LedColor::RED,
            context_menu: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn colors(&self) -> Option<&[LedColor]> {
        self.colors.as_deref()
    }

    /// Replaces the content (without emitting `Changed`).
    pub fn set_text(&mut self, text: &str, colors: Option<Vec<LedColor>>, cx: &mut Context<Self>) {
        if self.content.as_ref() != text || self.colors != colors {
            self.content = SharedString::from(text.to_string());
            let n = text.chars().count();
            self.colors = colors.map(|mut c| {
                c.resize(n, self.current_color);
                c
            });
            let end = self.content.len();
            self.selected_range = self.selected_range.start.min(end)..self.selected_range.end.min(end);
            if !self.content.is_char_boundary(self.selected_range.start)
                || !self.content.is_char_boundary(self.selected_range.end)
            {
                self.selected_range = end..end;
            }
            self.marked_range = None;
            cx.notify();
        }
    }

    pub fn set_mono_tint(&mut self, c: LedColor) {
        self.mono_tint = c;
    }

    pub fn current_color(&self) -> LedColor {
        self.current_color
    }

    /// Sets the color for newly typed text and recolors any selection.
    pub fn set_current_color(&mut self, c: LedColor, cx: &mut Context<Self>) {
        self.current_color = c;
        if let Some(colors) = self.colors.as_mut()
            && !self.selected_range.is_empty()
        {
            let a = self.content[..self.selected_range.start].chars().count();
            let b = self.content[..self.selected_range.end].chars().count();
            for slot in colors.iter_mut().take(b).skip(a) {
                *slot = c;
            }
            cx.emit(InputEvent::Changed);
        }
        cx.notify();
    }

    /// Inserts text at the cursor (used by the icon buttons).
    pub fn insert(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, text, window, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        if event.click_count >= 2 {
            self.selected_range = 0..self.content.len();
            cx.notify();
            return;
        }
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    /// Right-click outside the selection moves the caret there first.
    fn on_right_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        let at = self.index_for_mouse_position(event.position);
        if !(self.selected_range.start..=self.selected_range.end).contains(&at) || self.selected_range.is_empty() {
            self.move_to(at, cx);
        }
        self.context_menu = Some(event.position);
        cx.stop_propagation();
        cx.notify();
    }

    fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    /// The right-click menu at `at`, with a backdrop that closes it.
    fn render_context_menu(
        &self,
        at: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        use crate::widgets::{menu_item, menu_with_highlight};
        let has_selection = !self.selected_range.is_empty();
        let can_paste = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .is_some_and(|t| !t.is_empty());
        let m = if cfg!(target_os = "macos") { "\u{2318}" } else { "Ctrl+" };
        let this = cx.entity().downgrade();
        let run = |f: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            let this = this.clone();
            move |w: &mut Window, cx: &mut App| {
                this.update(cx, |t, cx| {
                    t.context_menu = None;
                    f(t, w, cx);
                    cx.notify();
                })
                .ok();
            }
        };
        let items = vec![
            menu_item("Cut", false, run(|t, w, cx| t.cut(&Cut, w, cx)))
                .hint(format!("{m}X"))
                .disabled(!has_selection),
            menu_item("Copy", false, run(|t, w, cx| t.copy(&Copy, w, cx)))
                .hint(format!("{m}C"))
                .disabled(!has_selection),
            menu_item("Paste", false, run(|t, w, cx| t.paste(&Paste, w, cx)))
                .hint(format!("{m}V"))
                .disabled(!can_paste),
            menu_item("Select All", false, run(|t, w, cx| t.select_all(&SelectAll, w, cx)))
                .hint(format!("{m}A"))
                .disabled(self.content.is_empty())
                .separator(),
        ];
        let close = cx.listener(|t, _: &MouseDownEvent, _, cx| t.close_context_menu(cx));
        let close_right = cx.listener(|t, _: &MouseDownEvent, _, cx| t.close_context_menu(cx));
        gpui::deferred(
            gpui::anchored().position(point(px(0.), px(0.))).child(
                div()
                    .id("text-context-backdrop")
                    .w(window.viewport_size().width)
                    .h(window.viewport_size().height)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, close)
                    .on_mouse_down(MouseButton::Right, close_right)
                    .child(
                        gpui::anchored()
                            .position(at)
                            .snap_to_window()
                            .child(menu_with_highlight("text-context-menu", items, 180., None)),
                    ),
            ),
        )
        .with_priority(3)
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace(['\n', '\r'], " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref()) else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left() + self.scroll_x)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    /// Replaces `range` (bytes) with `new_text`, keeping colors aligned.
    fn splice(&mut self, range: Range<usize>, new_text: &str) {
        if let Some(colors) = self.colors.as_mut() {
            let a = self.content[..range.start].chars().count();
            let removed = self.content[range.clone()].chars().count();
            let added = new_text.chars().count();
            let a = a.min(colors.len());
            let b = (a + removed).min(colors.len());
            colors.splice(a..b, std::iter::repeat_n(self.current_color, added));
        }
        self.content = (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..]).into();
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let new_text = new_text.replace(['\n', '\r'], "");
        self.splice(range.clone(), &new_text);
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        self.splice(range.clone(), new_text);
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        // The IME's selection is in UTF-16 units within `new_text`.
        let in_new = |o: usize| utf16_to_utf8(new_text, o);
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| in_new(r.start)..in_new(r.end))
            .map_or_else(
                || range.start + new_text.len()..range.start + new_text.len(),
                |new_range| new_range.start + range.start..new_range.end + range.start,
            );
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start) - self.scroll_x,
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end) - self.scroll_x,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(line_point.x + self.scroll_x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

/// Byte offset in `s` of the UTF-16 offset `utf16` (clamped to the end).
fn utf16_to_utf8(s: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, ch) in s.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += ch.len_utf16();
    }
    s.len()
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    /// The whole line in one style: positions for the cursor, selection
    /// and clicks.
    line: Option<ShapedLine>,
    /// Each differently colored stretch, with its x position on `line`.
    /// Painted separately because GPUI can put later runs a pixel lower.
    pieces: Vec<(Pixels, ShapedLine)>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll_x: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let base = TextRun {
            len: 0,
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let (display_text, mut runs) = if content.is_empty() {
            (
                input.placeholder.clone(),
                vec![TextRun {
                    len: input.placeholder.len(),
                    color: theme::text_dim().into(),
                    ..base
                }],
            )
        } else if let Some(colors) = &input.colors {
            let mut runs: Vec<TextRun> = Vec::new();
            for (i, ch) in content.chars().enumerate() {
                let c = colors.get(i).copied().unwrap_or(input.current_color);
                let color = theme::text_color_for(c);
                match runs.last_mut() {
                    Some(r) if r.color == color => r.len += ch.len_utf8(),
                    _ => runs.push(TextRun {
                        len: ch.len_utf8(),
                        color,
                        ..base.clone()
                    }),
                }
            }
            (content.clone(), runs)
        } else {
            (
                content.clone(),
                vec![TextRun {
                    len: content.len(),
                    ..base
                }],
            )
        };
        if let Some(marked) = input.marked_range.as_ref() {
            // Underline the IME composition range.
            let mut out = Vec::new();
            let mut pos = 0;
            for r in runs.drain(..) {
                let (start, end) = (pos, pos + r.len);
                pos = end;
                let cuts = [start, marked.start.clamp(start, end), marked.end.clamp(start, end), end];
                for &[from, to] in cuts.array_windows() {
                    if to > from {
                        let underlined = from >= marked.start && to <= marked.end;
                        out.push(TextRun {
                            len: to - from,
                            underline: underlined.then(|| UnderlineStyle {
                                color: Some(r.color),
                                thickness: px(1.0),
                                wavy: false,
                            }),
                            ..r.clone()
                        });
                    }
                }
            }
            runs = out;
        }

        let font_size = style.font_size.to_pixels(window.rem_size());
        let plain = TextRun {
            len: display_text.len(),
            ..runs[0].clone()
        };
        let line = window
            .text_system()
            .shape_line(display_text.clone(), font_size, &[plain], None);
        let mut pieces = Vec::with_capacity(runs.len());
        let mut start = 0;
        for run in &runs {
            let end = start + run.len;
            let text: SharedString = display_text[start..end].to_string().into();
            let piece = window
                .text_system()
                .shape_line(text, font_size, std::slice::from_ref(run), None);
            pieces.push((line.x_for_index(start), piece));
            start = end;
        }

        let cursor_pos = if content.is_empty() {
            px(0.)
        } else {
            line.x_for_index(cursor)
        };
        // Keep the cursor visible.
        let width = bounds.size.width;
        let mut scroll_x = input.scroll_x;
        if cursor_pos - scroll_x > width - px(4.) {
            scroll_x = cursor_pos - width + px(4.);
        }
        if cursor_pos < scroll_x {
            scroll_x = cursor_pos;
        }
        if line.width < width {
            scroll_x = px(0.);
        }
        let x0 = bounds.left() - scroll_x;

        let (selection, cursor) = if selected_range.is_empty() || content.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(point(x0 + cursor_pos, bounds.top()), size(px(1.5), bounds.size.height)),
                    theme::border_focus(),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(x0 + line.x_for_index(selected_range.start), bounds.top()),
                        point(x0 + line.x_for_index(selected_range.end), bounds.bottom()),
                    ),
                    theme::selection(),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            pieces,
            cursor,
            selection,
            scroll_x,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(&focus_handle, ElementInputHandler::new(bounds, self.input.clone()), cx);
        let line = prepaint.line.take().unwrap();
        let scroll_x = prepaint.scroll_x;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection);
            }
            for (x, piece) in &prepaint.pieces {
                piece
                    .paint(
                        point(bounds.left() - scroll_x + *x, bounds.top()),
                        window.line_height(),
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
            }
            if focus_handle.is_focused(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });
        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll_x = scroll_x;
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let menu = self.context_menu.map(|at| self.render_context_menu(at, window, cx));
        div()
            .flex()
            .key_context("TextInput")
            .on_key_down(cx.listener(|t, e: &gpui::KeyDownEvent, _, cx| {
                if t.context_menu.is_some() && e.keystroke.key == "escape" {
                    t.close_context_menu(cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .h(px(34.))
            .px_2()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(if focused {
                theme::border_focus()
            } else {
                theme::border()
            })
            .bg(theme::panel_alt())
            .text_color(theme::text())
            .text_size(px(15.))
            .line_height(px(22.))
            .child(TextElement { input: cx.entity() })
            .children(menu)
    }
}

/// Key bindings for text inputs.
pub fn bindings() -> Vec<gpui::KeyBinding> {
    use gpui::KeyBinding;
    let m = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };
    let ctx = Some("TextInput");
    vec![
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new(&format!("{m}-a"), SelectAll, ctx),
        KeyBinding::new(&format!("{m}-v"), Paste, ctx),
        KeyBinding::new(&format!("{m}-c"), Copy, ctx),
        KeyBinding::new(&format!("{m}-x"), Cut, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
    ]
}
