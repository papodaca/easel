#![allow(dead_code)]

use gpui::{
    Anchor, App, Context, FocusHandle, Focusable, MouseButton, SharedString, Window, anchored,
    deferred, div, prelude::*, px, rgb,
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
            on_change: None,
        }
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn set_value(&mut self, value: Value, cx: &mut Context<Self>) {
        self.value = value;
        cx.notify();
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
            theme::BUTTON_HOVER
        } else {
            theme::MUTED
        };
        let menu = self.open.then(|| {
            let highlight = self.highlight;
            let options = self.options.clone();
            deferred(
                anchored()
                    .anchor(Anchor::TopLeft)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .w(px(220.0))
                            .bg(rgb(theme::SIDE_PANEL))
                            .text_color(rgb(theme::TEXT))
                            .border_1()
                            .border_color(rgb(theme::BUTTON_HOVER))
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.open = false;
                                cx.notify();
                            }))
                            .children(options.into_iter().enumerate().map(|(index, option)| {
                                let selected = index == highlight;
                                div()
                                    .px(px(8.0))
                                    .py(px(6.0))
                                    .bg(rgb(if selected {
                                        theme::BUTTON
                                    } else {
                                        theme::SIDE_PANEL
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
        field_row(
            self.label.as_deref(),
            self.help.as_deref(),
            div()
                .id("select")
                .key_context("Select")
                .track_focus(&self.focus_handle)
                .relative()
                .w_full()
                .min_h(px(36.0))
                .px(px(8.0))
                .py(px(6.0))
                .bg(rgb(theme::FIELD))
                .text_color(rgb(theme::TEXT))
                .border_1()
                .border_color(rgb(border))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(current)
                .children(menu),
        )
    }
}

pub fn bind_keys(_cx: &mut App) {}
