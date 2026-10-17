#![allow(dead_code)]

use gpui::{
    Anchor, App, Bounds, Context, FocusHandle, Focusable, MouseButton, Pixels, SharedString,
    Window, anchored, canvas, deferred, div, prelude::*, px, rgb,
};
use serde_json::Value;

use crate::theme;
use crate::widgets::field_row;

pub struct Select {
    focus_handle: FocusHandle,
    options: Vec<SharedString>,
    value: Value,
    open: bool,
    highlight: usize,
    label: Option<String>,
    help: Option<String>,
    enabled: bool,
    bounds: Bounds<Pixels>,
    on_change: Option<Box<dyn Fn(&Value, &mut Window, &mut App) + 'static>>,
}

impl Select {
    pub fn new(options: Vec<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            options,
            value: Value::Null,
            open: false,
            highlight: 0,
            label: None,
            help: None,
            enabled: true,
            bounds: Bounds::default(),
            on_change: None,
        }
    }

    pub fn set_label(&mut self, label: impl Into<String>, cx: &mut Context<Self>) {
        self.label = Some(label.into());
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_open(&self) -> bool {
        self.open
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn click_point(&self) -> gpui::Point<Pixels> {
        self.bounds.center()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn menu_point(&self, row: usize) -> gpui::Point<Pixels> {
        gpui::point(
            self.bounds.left() + px(16.0),
            self.bounds.bottom() + px(16.0) + px(28.0) * row as f32,
        )
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn set_value(&mut self, value: Value, cx: &mut Context<Self>) {
        self.value = value;
        cx.notify();
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
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

    fn open_menu(&mut self, cx: &mut Context<Self>) {
        if !self.enabled || self.options.is_empty() {
            return;
        }
        self.highlight = self
            .options
            .iter()
            .position(|option| self.value.as_str() == Some(option.as_ref()))
            .unwrap_or(0);
        self.open = true;
        cx.notify();
    }

    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(option) = self.options.get(self.highlight).cloned() else {
            return;
        };
        let value = Value::String(option.to_string());
        let changed = self.value != value;
        self.value = value.clone();
        self.open = false;
        if changed && let Some(callback) = self.on_change.as_ref() {
            callback(&value, window, cx);
        }
        cx.notify();
    }

    fn on_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" | "space" if !self.open => self.open_menu(cx),
            "escape" if self.open => {
                self.open = false;
                cx.notify();
            }
            "down" if self.open => {
                self.highlight = (self.highlight + 1).min(self.options.len().saturating_sub(1));
                cx.notify();
            }
            "up" if self.open => {
                self.highlight = self.highlight.saturating_sub(1);
                cx.notify();
            }
            "enter" if self.open => self.choose(window, cx),
            _ => {}
        }
    }
}

impl Focusable for Select {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl gpui::Render for Select {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let current = self
            .value
            .as_str()
            .map(|text| text.to_owned())
            .unwrap_or_else(|| "Select".to_owned());
        let focused = self.focus_handle.is_focused(window);
        let border = if focused {
            theme::BORDER_FOCUSED
        } else {
            theme::BORDER
        };
        let menu = self.open.then(|| {
            let highlight = self.highlight;
            let options = self.options.clone();
            let width = self.bounds.size.width.max(px(160.0));
            deferred(
                anchored()
                    .anchor(Anchor::TopLeft)
                    .position(gpui::point(self.bounds.left(), self.bounds.bottom()))
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .w(width)
                            .py(px(4.0))
                            .rounded_md()
                            .shadow_md()
                            .bg(rgb(theme::SURFACE))
                            .text_color(rgb(theme::TEXT))
                            .border_1()
                            .border_color(rgb(theme::BORDER))
                            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                if this.bounds.contains(&event.position) {
                                    return;
                                }
                                this.open = false;
                                cx.notify();
                            }))
                            .children(options.into_iter().enumerate().map(|(index, option)| {
                                let selected = index == highlight;
                                div()
                                    .mx(px(4.0))
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded_sm()
                                    .bg(rgb(if selected {
                                        theme::ELEMENT_HOVER
                                    } else {
                                        theme::SURFACE
                                    }))
                                    .child(option.to_string())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _, window, cx| {
                                            this.highlight = index;
                                            this.choose(window, cx);
                                        }),
                                    )
                            })),
                    ),
            )
        });
        let bounds_entity = cx.entity();
        field_row(
            self.label.as_deref(),
            self.help.as_deref(),
            div()
                .id(("select", cx.entity_id()))
                .key_context("Select")
                .track_focus(&self.focus_handle)
                .relative()
                .w_full()
                .min_w(px(0.0))
                .min_h(px(30.0))
                .px(px(8.0))
                .py(px(5.0))
                .rounded_sm()
                .bg(rgb(theme::FIELD))
                .text_color(rgb(theme::TEXT))
                .border_1()
                .border_color(rgb(border))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        if !this.enabled {
                            return;
                        }
                        this.focus_handle.focus(window, cx);
                        if this.open {
                            this.open = false;
                            cx.notify();
                        } else {
                            this.open_menu(cx);
                        }
                    }),
                )
                .on_key_down(cx.listener(Self::on_key_down))
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            bounds_entity.update(cx, |this, _| this.bounds = bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(div().overflow_hidden().w_full().child(current))
                .children(menu),
        )
    }
}

pub fn bind_keys(_cx: &mut App) {}
