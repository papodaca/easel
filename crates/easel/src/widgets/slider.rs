#![allow(dead_code)]

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, MouseButton, MouseMoveEvent, Pixels, Window,
    canvas, div, prelude::*, px, rgb,
};
use serde_json::{Value, json};

use crate::theme;
use crate::widgets::field_row;

pub struct Slider {
    focus_handle: FocusHandle,
    min: f64,
    max: f64,
    value: f64,
    label: Option<String>,
    help: Option<String>,
    enabled: bool,
    dragging: bool,
    track_bounds: Bounds<Pixels>,
    on_change: Option<Box<dyn Fn(&Value, &mut Window, &mut App) + 'static>>,
}

impl Slider {
    pub fn new(min: f64, max: f64, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            min,
            max,
            value: min,
            label: None,
            help: None,
            enabled: true,
            dragging: false,
            track_bounds: Bounds::default(),
            on_change: None,
        }
    }

    pub fn value(&self) -> Value {
        json!(self.value)
    }

    pub fn set_range(&mut self, min: f64, max: f64, cx: &mut Context<Self>) {
        self.min = min;
        self.max = max;
        self.value = self.value.clamp(self.min, self.max);
        cx.notify();
    }

    pub fn max_value(&self) -> f64 {
        self.max
    }

    pub fn set_value(&mut self, value: &Value, cx: &mut Context<Self>) {
        if let Some(number) = value.as_f64() {
            self.value = number.clamp(self.min, self.max);
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

    pub fn track_bounds(&self) -> Bounds<Pixels> {
        self.track_bounds
    }

    fn set_from_x(&mut self, x: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        let left = self.track_bounds.left().as_f32();
        let right = self.track_bounds.right().as_f32();
        let width = (right - left).max(1.0);
        let x = x.as_f32();
        let t = if x >= right - 1.5 {
            1.0
        } else {
            ((x - left) / width).clamp(0.0, 1.0)
        } as f64;
        self.value = self.min + (self.max - self.min) * t;
        let value = json!(self.value);
        if let Some(callback) = self.on_change.as_ref() {
            callback(&value, window, cx);
        }
        cx.notify();
    }

    fn on_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dragging = true;
        self.set_from_x(event.position.x, window, cx);
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dragging {
            self.set_from_x(event.position.x, window, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &gpui::MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.dragging = false;
    }
}

impl Focusable for Slider {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl gpui::Render for Slider {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let bounds_entity = cx.entity();
        let fraction = {
            let span = (self.max - self.min).max(f64::EPSILON);
            ((self.value - self.min) / span).clamp(0.0, 1.0)
        };
        field_row(
            self.label.as_deref(),
            self.help.as_deref(),
            div()
                .id("slider")
                .track_focus(&self.focus_handle)
                .relative()
                .w_full()
                .h(px(28.0))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                .on_mouse_move(cx.listener(Self::on_mouse_move))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            bounds_entity.update(cx, |this, _| this.track_bounds = bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(12.0))
                        .h(px(4.0))
                        .w_full()
                        .rounded_full()
                        .bg(rgb(theme::ELEMENT)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(12.0))
                        .h(px(4.0))
                        .w(gpui::relative(fraction as f32))
                        .rounded_full()
                        .bg(rgb(theme::ACCENT)),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(7.0))
                        .left(gpui::relative(fraction as f32))
                        .ml(px(-7.0))
                        .size(px(14.0))
                        .rounded_full()
                        .bg(rgb(theme::TEXT))
                        .border_1()
                        .border_color(rgb(theme::BORDER)),
                ),
        )
    }
}
