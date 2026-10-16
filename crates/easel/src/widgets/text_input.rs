#![allow(dead_code)]

// Adapted from Zed `crates/gpui/examples/input.rs` at v1.24.2.
// Copyright 2022-2025 Zed Industries, Inc.
// Licensed under the Apache License, Version 2.0.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, IntoElement,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, ShapedLine, SharedString, Style, Subscription, TextRun, UTF16Selection,
    UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, relative, rgb, size,
};
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;

use crate::theme;
use crate::widgets::field_row;

actions!(
    easel_field,
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
        Copy,
        Up,
        Down,
        Newline,
    ]
);

pub fn bind_keys(cx: &mut App) {
    for context in ["TextInput", "TextArea", "NumberInput"] {
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some(context)),
            KeyBinding::new("delete", Delete, Some(context)),
            KeyBinding::new("left", Left, Some(context)),
            KeyBinding::new("right", Right, Some(context)),
            KeyBinding::new("shift-left", SelectLeft, Some(context)),
            KeyBinding::new("shift-right", SelectRight, Some(context)),
            KeyBinding::new("ctrl-a", SelectAll, Some(context)),
            KeyBinding::new("ctrl-v", Paste, Some(context)),
            KeyBinding::new("ctrl-c", Copy, Some(context)),
            KeyBinding::new("ctrl-x", Cut, Some(context)),
            KeyBinding::new("home", Home, Some(context)),
            KeyBinding::new("end", End, Some(context)),
        ]);
    }
}

pub enum TextEvent {
    Submit,
    Blur,
}

type Callback = Box<dyn Fn(&mut Window, &mut App) + 'static>;

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    key_context: &'static str,
    label: Option<String>,
    help: Option<String>,
    enabled: bool,
    multiline: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    preferred_column: usize,
    on_change: Option<Box<dyn Fn(&Value, &mut Window, &mut App) + 'static>>,
    on_submit: Option<Callback>,
    on_blur: Option<Callback>,
    focus_subscription: Option<Subscription>,
}

impl TextInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::build(cx, false, "TextInput")
    }

    pub fn multiline(cx: &mut Context<Self>) -> Self {
        Self::build(cx, true, "TextArea")
    }

    fn build(cx: &mut Context<Self>, multiline: bool, key_context: &'static str) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: SharedString::default(),
            key_context,
            label: None,
            help: None,
            enabled: true,
            multiline,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            preferred_column: 0,
            on_change: None,
            on_submit: None,
            on_blur: None,
            focus_subscription: None,
        }
    }

    pub fn with_key_context(mut self, key_context: &'static str) -> Self {
        self.key_context = key_context;
        self
    }

    pub fn text(&self) -> &str {
        self.content.as_ref()
    }

    pub fn value(&self) -> Value {
        Value::String(self.content.to_string())
    }

    pub fn set_value(&mut self, value: &Value, cx: &mut Context<Self>) {
        if let Some(text) = value.as_str() {
            self.set_text(text, cx);
        }
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.content = text.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.marked_range = None;
        self.preferred_column = self.column_at(end);
        cx.notify();
    }

    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }

    pub fn set_label(&mut self, label: impl Into<String>, cx: &mut Context<Self>) {
        self.label = Some(label.into());
        cx.notify();
    }

    pub fn set_help(&mut self, help: impl Into<String>, cx: &mut Context<Self>) {
        self.help = Some(help.into());
        cx.notify();
    }

    pub fn on_change(
        &mut self,
        callback: Box<dyn Fn(&Value, &mut Window, &mut App) + 'static>,
        cx: &mut Context<Self>,
    ) {
        self.on_change = Some(callback);
        cx.notify();
    }

    pub fn on_submit_callback(&mut self, callback: Callback) {
        self.on_submit = Some(callback);
    }

    pub fn on_blur_callback(&mut self, callback: Callback) {
        self.on_blur = Some(callback);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        let offset = if self.selected_range.is_empty() {
            self.previous_boundary(self.cursor_offset())
        } else {
            self.selected_range.start
        };
        self.move_to(offset, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        let offset = if self.selected_range.is_empty() {
            self.next_boundary(self.selected_range.end)
        } else {
            self.selected_range.end
        };
        self.move_to(offset, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.enabled {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.enabled {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        if self.enabled {
            self.move_to(self.line_start(self.cursor_offset()), cx);
        }
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        if self.enabled {
            self.move_to(self.line_end(self.cursor_offset()), cx);
        }
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = if self.multiline {
                text
            } else {
                text.replace(['\n', '\r'], " ")
            };
            self.replace_text_in_range(None, &text, window, cx);
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
        if self.enabled && !self.selected_range.is_empty() {
            self.copy(&Copy, window, cx);
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.enabled && self.multiline {
            self.move_vertical(-1, cx);
        }
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.enabled && self.multiline {
            self.move_vertical(1, cx);
        }
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.enabled && self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "enter" && !self.multiline {
            if let Some(callback) = self.on_submit.as_ref() {
                callback(window, cx);
            }
            cx.emit(TextEvent::Submit);
        }
    }

    fn on_focus_out(
        &mut self,
        _: gpui::FocusOutEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(callback) = self.on_blur.as_ref() {
            callback(window, cx);
        }
        cx.emit(TextEvent::Blur);
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.preferred_column = self.column_at(offset);
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
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.preferred_column = self.column_at(self.cursor_offset());
        cx.notify();
    }

    fn move_vertical(&mut self, delta: isize, cx: &mut Context<Self>) {
        let spans = line_spans(self.content.as_ref());
        let current = line_index(&spans, self.cursor_offset());
        let next = (current as isize + delta).clamp(0, spans.len() as isize - 1) as usize;
        let (start, end) = spans[next];
        let column = self.preferred_column.min(end - start);
        self.selected_range = (start + column)..(start + column);
        cx.notify();
    }

    fn column_at(&self, offset: usize) -> usize {
        let spans = line_spans(self.content.as_ref());
        let (start, end) = spans[line_index(&spans, offset)];
        offset.clamp(start, end) - start
    }

    fn line_start(&self, offset: usize) -> usize {
        let spans = line_spans(self.content.as_ref());
        spans[line_index(&spans, offset)].0
    }

    fn line_end(&self, offset: usize) -> usize {
        let spans = line_spans(self.content.as_ref());
        spans[line_index(&spans, offset)].1
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

    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_column = self.column_at(self.cursor_offset());
        cx.notify();
        let value = self.value();
        if let Some(callback) = self.on_change.as_ref() {
            callback(&value, window, cx);
        }
    }
}

impl EventEmitter<TextEvent> for TextInput {}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        let new_text = if self.multiline {
            new_text.to_owned()
        } else {
            new_text.replace(['\n', '\r'], "")
        };
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.content =
            (self.content[0..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.marked_range.take();
        self.changed(window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if new_text.is_empty() {
            self.marked_range = None;
        } else {
            self.marked_range = Some(range.start..range.start + new_text.len());
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| {
                let cursor = range.start + new_text.len();
                cursor..cursor
            });
        self.changed(window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    lines: Vec<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
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
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let multiline = self.input.read(cx).multiline;
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = if multiline {
            (window.line_height() * 4.0).into()
        } else {
            window.line_height().into()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let color = style.color;
        let logical: Vec<(usize, &str)> = {
            let spans = line_spans(content.as_ref());
            spans
                .into_iter()
                .map(|(start, end)| (start, &content[start..end]))
                .collect()
        };
        let mut lines = Vec::new();
        let mut cursor_origin = None;
        let mut selection = None;
        for (start, text) in logical {
            let display: SharedString = if text.is_empty() {
                " ".into()
            } else {
                text.to_owned().into()
            };
            let run = TextRun {
                len: display.len(),
                font: style.font(),
                color,
                background_color: None,
                underline: input.marked_range.as_ref().and_then(|marked| {
                    (marked.start >= start && marked.start < start + text.len()).then_some(
                        UnderlineStyle {
                            color: Some(color),
                            thickness: px(1.0),
                            wavy: false,
                        },
                    )
                }),
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .shape_line(display, font_size, &[run], None);
            if cursor >= start && cursor <= start + text.len() {
                let x = if text.is_empty() {
                    px(0.0)
                } else {
                    shaped.x_for_index(cursor - start)
                };
                let y = bounds.top() + window.line_height() * lines.len() as f32;
                cursor_origin = Some(point(bounds.left() + x, y));
            }
            if !selected_range.is_empty()
                && selected_range.start < start + text.len()
                && selected_range.end > start
            {
                let local_start = selected_range.start.saturating_sub(start).min(text.len());
                let local_end = selected_range.end.saturating_sub(start).min(text.len());
                let y = bounds.top() + window.line_height() * lines.len() as f32;
                selection = Some(fill(
                    Bounds::from_corners(
                        point(bounds.left() + shaped.x_for_index(local_start), y),
                        point(
                            bounds.left() + shaped.x_for_index(local_end),
                            y + window.line_height(),
                        ),
                    ),
                    rgb(0x4c5b78),
                ));
            }
            lines.push(shaped);
        }
        let cursor = cursor_origin.map(|origin| {
            fill(
                Bounds::new(origin, size(px(2.0), window.line_height())),
                rgb(theme::TEXT),
            )
        });
        PrepaintState {
            lines,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line_height = window.line_height();
        for (index, line) in prepaint.lines.drain(..).enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * index as f32);
            line.paint(origin, line_height, gpui::TextAlign::Left, None, window, cx)
                .ok();
            if index == 0 {
                self.input.update(cx, |input, _| {
                    input.last_layout = Some(line);
                    input.last_bounds = Some(bounds);
                });
            }
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_subscription.is_none() {
            let handle = self.focus_handle.clone();
            self.focus_subscription = Some(cx.on_focus_out(&handle, window, Self::on_focus_out));
        }
        let focused = self.focus_handle.is_focused(window);
        let border = if focused {
            theme::BUTTON_HOVER
        } else {
            theme::MUTED
        };
        field_row(
            self.label.as_deref(),
            self.help.as_deref(),
            div()
                .key_context(self.key_context)
                .track_focus(&self.focus_handle)
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
                .on_action(cx.listener(Self::up))
                .on_action(cx.listener(Self::down))
                .on_action(cx.listener(Self::newline))
                .on_key_down(cx.listener(Self::on_key_down))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                .on_mouse_move(cx.listener(Self::on_mouse_move))
                .w_full()
                .bg(rgb(theme::FIELD))
                .text_color(rgb(theme::TEXT))
                .text_sm()
                .px(px(8.0))
                .py(px(6.0))
                .border_1()
                .border_color(rgb(border))
                .child(TextElement { input: cx.entity() }),
        )
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub(crate) fn line_spans(content: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for (idx, ch) in content.char_indices() {
        if ch == '\n' {
            spans.push((start, idx));
            start = idx + ch.len_utf8();
        }
    }
    spans.push((start, content.len()));
    if spans.is_empty() {
        spans.push((0, 0));
    }
    spans
}

fn line_index(spans: &[(usize, usize)], offset: usize) -> usize {
    spans
        .iter()
        .position(|(start, end)| offset >= *start && offset <= *end)
        .unwrap_or(spans.len().saturating_sub(1))
}
