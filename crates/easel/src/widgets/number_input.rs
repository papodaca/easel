#![allow(dead_code)]

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, MouseButton, Pixels, Window, canvas, div, point,
    prelude::*, px, rgb,
};
use serde_json::{Value, json};

use crate::theme;
use crate::widgets::field_row;
use crate::widgets::text_input::TextInput;

pub struct NumberInput {
    input: gpui::Entity<TextInput>,
    min: f64,
    max: f64,
    step: f64,
    value: f64,
    label: Option<String>,
    help: Option<String>,
    enabled: bool,
    step_up_bounds: Bounds<Pixels>,
    on_change: Option<Box<dyn Fn(&Value, &mut Window, &mut App) + 'static>>,
}

impl NumberInput {
    pub fn new(min: f64, max: f64, step: f64, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(cx).with_key_context("NumberInput"));
        let weak = cx.weak_entity();
        input.update(cx, |input, _| {
            let submit = weak.clone();
            input.on_submit_callback(Box::new(move |window, cx| {
                submit.update(cx, |this, cx| this.commit(window, cx)).ok();
            }));
            let blur = weak.clone();
            input.on_blur_callback(Box::new(move |window, cx| {
                blur.update(cx, |this, cx| this.commit(window, cx)).ok();
            }));
        });
        Self {
            input,
            min,
            max,
            step,
            value: min,
            label: None,
            help: None,
            enabled: true,
            step_up_bounds: Bounds::default(),
            on_change: None,
        }
    }

    pub fn value(&self) -> Value {
        json!(self.value)
    }

    pub fn set_value(&mut self, value: &Value, cx: &mut Context<Self>) {
        if let Some(number) = value.as_f64() {
            self.value = number.clamp(self.min, self.max);
            let text = format_number(self.value);
            self.input.update(cx, |input, cx| input.set_text(&text, cx));
        }
    }

    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        self.input
            .update(cx, |input, cx| input.set_enabled(enabled, cx));
    }

    pub fn on_change(
        &mut self,
        callback: Box<dyn Fn(&Value, &mut Window, &mut App) + 'static>,
        cx: &mut Context<Self>,
    ) {
        self.on_change = Some(callback);
        cx.notify();
    }

    pub fn step_up_point(&self) -> gpui::Point<Pixels> {
        point(
            self.step_up_bounds.left() + self.step_up_bounds.size.width / 2.0,
            self.step_up_bounds.top() + self.step_up_bounds.size.height / 2.0,
        )
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().trim().to_owned();
        if text.is_empty() {
            return;
        }
        match text.parse::<f64>() {
            Ok(parsed) => {
                let clamped = parsed.clamp(self.min, self.max);
                let changed = (clamped - self.value).abs() > f64::EPSILON;
                self.value = clamped;
                let rendered = format_number(clamped);
                if self.input.read(cx).text() != rendered {
                    self.input
                        .update(cx, |input, cx| input.set_text(&rendered, cx));
                }
                if changed {
                    self.fire(window, cx);
                }
            }
            Err(_) => {
                let rendered = format_number(self.value);
                if self.input.read(cx).text() != rendered {
                    self.input
                        .update(cx, |input, cx| input.set_text(&rendered, cx));
                }
            }
        }
    }

    fn step_by(&mut self, direction: f64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        let next = (self.value + direction * self.step).clamp(self.min, self.max);
        if (next - self.value).abs() <= f64::EPSILON {
            return;
        }
        self.value = next;
        let rendered = format_number(next);
        self.input
            .update(cx, |input, cx| input.set_text(&rendered, cx));
        self.fire(window, cx);
    }

    fn fire(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = json!(self.value);
        if let Some(callback) = self.on_change.as_ref() {
            callback(&value, window, cx);
        }
        cx.notify();
    }
}

impl Focusable for NumberInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl gpui::Render for NumberInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if !self.input.focus_handle(cx).is_focused(window) {
            self.commit(window, cx);
        }
        let step_entity = cx.entity();
        field_row(
            self.label.as_deref(),
            self.help.as_deref(),
            div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .w_full()
                .items_center()
                .child(div().flex_1().min_w(px(0.0)).child(self.input.clone()))
                .child(
                    div()
                        .id("number-step-up")
                        .relative()
                        .w(px(32.0))
                        .h(px(32.0))
                        .flex_none()
                        .bg(rgb(theme::BUTTON))
                        .text_color(rgb(theme::TEXT))
                        .child(
                            canvas(
                                move |bounds, _, cx| {
                                    step_entity.update(cx, |this, _| this.step_up_bounds = bounds);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child("+")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        this.step_by(1.0, window, cx)
                                    }),
                                ),
                        ),
                ),
        )
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}
