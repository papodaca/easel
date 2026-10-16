#![allow(dead_code)]

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, MouseButton, Pixels, Window, canvas, div, point,
    prelude::*, px, rgb,
};
use serde_json::{Value, json};

use crate::theme;
use crate::widgets::field_row;

pub struct Checkbox {
    focus_handle: FocusHandle,
    checked: bool,
    label: Option<String>,
    help: Option<String>,
    enabled: bool,
    bounds: Bounds<Pixels>,
    on_change: Option<Box<dyn Fn(&Value, &mut Window, &mut App) + 'static>>,
}

impl Checkbox {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            checked: false,
            label: None,
            help: None,
            enabled: true,
            bounds: Bounds::default(),
            on_change: None,
        }
    }

    pub fn value(&self) -> Value {
        json!(self.checked)
    }

    pub fn set_value(&mut self, value: &Value, cx: &mut Context<Self>) {
        if let Some(checked) = value.as_bool() {
            self.checked = checked;
            cx.notify();
        }
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

    pub fn click_point(&self) -> gpui::Point<Pixels> {
        point(
            self.bounds.left() + self.bounds.size.width / 2.0,
            self.bounds.top() + self.bounds.size.height / 2.0,
        )
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        self.checked = !self.checked;
        let value = json!(self.checked);
        if let Some(callback) = self.on_change.as_ref() {
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
        if event.keystroke.key == "space" {
            self.toggle(window, cx);
        }
    }
}

impl Focusable for Checkbox {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl gpui::Render for Checkbox {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let bounds_entity = cx.entity();
        let focused = self.focus_handle.is_focused(window);
        let mark = if self.checked { "✓" } else { "" };
        let border = if focused {
            theme::BUTTON_HOVER
        } else {
            theme::MUTED
        };
        field_row(
            self.label.as_deref(),
            self.help.as_deref(),
            div()
                .id("checkbox")
                .track_focus(&self.focus_handle)
                .relative()
                .size(px(28.0))
                .bg(rgb(theme::FIELD))
                .text_color(rgb(theme::TEXT))
                .border_2()
                .border_color(rgb(border))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.toggle(window, cx)),
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
                .child(mark),
        )
    }
}
