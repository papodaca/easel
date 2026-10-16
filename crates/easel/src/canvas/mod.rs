mod viewport;

use easel_core::geometry::{Point, Viewport};
use gpui::{
    App, Bounds, Context, CursorStyle, FocusHandle, Focusable, IntoElement, MouseButton,
    MouseMoveEvent, ParentElement, Pixels, Render, ScrollDelta, ScrollWheelEvent, Styled, Window,
    canvas, div, img, prelude::*, px, rgb,
};

use crate::actions::{ActualSize, Fit};
use crate::app_state::AppState;
use crate::image_load::DecodedImage;
use crate::notifications::Notifications;
use crate::theme;

pub struct Canvas {
    focus_handle: FocusHandle,
    app_state: gpui::Entity<AppState>,
    notifications: gpui::Entity<Notifications>,
    viewport: Viewport,
    bounds: Bounds<Pixels>,
    user_zoomed: bool,
    space_held: bool,
    last_cursor: Option<gpui::Point<Pixels>>,
    shown_image: Option<u64>,
}

impl Canvas {
    pub fn new(
        app_state: gpui::Entity<AppState>,
        notifications: gpui::Entity<Notifications>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            app_state,
            notifications,
            viewport: Viewport::fit(
                easel_core::geometry::Size { w: 0, h: 0 },
                easel_core::geometry::Size { w: 0, h: 0 },
            ),
            bounds: Bounds::default(),
            user_zoomed: false,
            space_held: false,
            last_cursor: None,
            shown_image: None,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn app_state(&self) -> &gpui::Entity<AppState> {
        &self.app_state
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn content_bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.user_zoomed = false;
        self.refit(cx);
        cx.notify();
    }

    pub fn actual_size(&mut self, cx: &mut Context<Self>) {
        self.user_zoomed = true;
        self.viewport.actual_size();
        cx.notify();
    }

    fn refit(&mut self, cx: &mut Context<Self>) {
        let Some(image) = self.app_state.read(cx).image.clone() else {
            return;
        };
        self.viewport = Viewport::fit(image.size, self.viewport.view_size);
    }

    fn note_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let view_size =
            viewport::size_from_pixels(bounds.size.width.as_f32(), bounds.size.height.as_f32());
        if self.bounds == bounds && self.viewport.view_size == view_size {
            return;
        }
        self.bounds = bounds;
        self.viewport.view_size = view_size;
        if self.user_zoomed {
            self.viewport.clamp();
        } else {
            self.refit(cx);
        }
        cx.notify();
    }

    fn sync_image(&mut self, image: Option<&DecodedImage>) {
        let id = image.map(|image| image.render.id.0 as u64);
        if id == self.shown_image {
            return;
        }
        self.shown_image = id;
        self.user_zoomed = false;
        if let Some(image) = image {
            self.viewport = Viewport::fit(image.size, self.viewport.view_size);
        }
    }

    fn zoom_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let local = Point {
            x: (event.position.x - self.bounds.origin.x).as_f32(),
            y: (event.position.y - self.bounds.origin.y).as_f32(),
        };
        self.viewport
            .zoom_at(local, scroll_zoom_factor(event.delta));
        self.user_zoomed = true;
        cx.notify();
    }

    fn pan_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let panning = event.pressed_button == Some(MouseButton::Middle)
            || (self.space_held && event.pressed_button == Some(MouseButton::Left));
        if !panning {
            self.last_cursor = None;
            return;
        }
        if let Some(last) = self.last_cursor {
            self.viewport.pan_by(Point {
                x: (event.position.x - last.x).as_f32(),
                y: (event.position.y - last.y).as_f32(),
            });
            self.user_zoomed = true;
            cx.notify();
        }
        self.last_cursor = Some(event.position);
    }
}

fn scroll_zoom_factor(delta: ScrollDelta) -> f32 {
    let lines = match delta {
        ScrollDelta::Lines(delta) => delta.y,
        ScrollDelta::Pixels(delta) => delta.y.as_f32() / 40.0,
    };
    1.1_f32.powf(-lines)
}

impl Focusable for Canvas {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Canvas {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let image = self.app_state.read(cx).image.clone();
        self.sync_image(image.as_deref());
        let image_layer = match &image {
            Some(image) => {
                let rect = self.viewport.image_rect_in_view();
                img(image.render.clone())
                    .absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.w.max(0.0)))
                    .h(px(rect.h.max(0.0)))
                    .into_any_element()
            }
            None => div()
                .size_full()
                .p(px(12.))
                .text_color(rgb(theme::TEXT))
                .child("Canvas")
                .into_any_element(),
        };
        let bounds_entity = cx.entity();
        div()
            .id("canvas")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Fit, _, cx| this.fit(cx)))
            .on_action(cx.listener(|this, _: &ActualSize, _, cx| this.actual_size(cx)))
            .relative()
            .flex_1()
            .h_full()
            .min_w(px(0.))
            .overflow_hidden()
            .bg(rgb(theme::CANVAS))
            .text_color(rgb(theme::TEXT))
            .child(image_layer)
            .child(
                canvas(
                    move |bounds, _window, cx| {
                        bounds_entity.update(cx, |canvas, cx| canvas.note_bounds(bounds, cx));
                    },
                    |_bounds, (), _window, _cx| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id("canvas-hit")
                    .absolute()
                    .size_full()
                    .cursor(CursorStyle::Crosshair)
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                        this.zoom_wheel(event, cx);
                    }))
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        this.pan_move(event, cx);
                    }))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, _| {
                        if event.keystroke.key == "space" {
                            this.space_held = true;
                        }
                    }))
                    .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, _, _| {
                        if event.keystroke.key == "space" {
                            this.space_held = false;
                        }
                    })),
            )
            .child(self.notifications.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use easel_core::geometry::{Point, Size, Viewport};
    use gpui::{
        Focusable as _, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, point,
        prelude::*, px,
    };

    use crate::actions::{ActualSize, Fit};
    use crate::app_state::AppState;
    use crate::image_load::decode_image;
    use crate::notifications::Notifications;

    use super::Canvas;

    fn large_image() -> Arc<crate::image_load::DecodedImage> {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/pixel.png"
        ))
        .unwrap();
        let mut image = decode_image(&bytes, "pixel.png").unwrap();
        image.size = Size { w: 4000, h: 2000 };
        Arc::new(image)
    }

    fn open(cx: &mut TestAppContext) -> (gpui::WindowHandle<Canvas>, VisualTestContext) {
        let window = cx.update(|cx| {
            cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let state = cx.new(|_| AppState::new());
                let notes = cx.new(|_| Notifications::new());
                let canvas = cx.new(|cx| Canvas::new(state, notes, cx));
                canvas.focus_handle(cx).focus(window, cx);
                canvas
            })
            .unwrap()
        });
        let visual = VisualTestContext::from_window(window.into(), cx);
        (window, visual)
    }

    #[gpui::test]
    fn loaded_image_uses_the_fit_scale(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx);
        let canvas = window.root(&mut cx).unwrap();
        let image = large_image();
        canvas.update(&mut cx, |canvas, cx| {
            canvas
                .app_state()
                .update(cx, |state, cx| state.set_image(image, cx));
        });
        let viewport = canvas.read_with(&cx, |canvas, _| canvas.viewport());
        let expected = Viewport::fit(viewport.image_size, viewport.view_size);
        assert!(
            viewport.view_size.w > 0 && viewport.view_size.h > 0,
            "canvas never received a size"
        );
        assert!(
            (viewport.scale - expected.scale).abs() < 1e-3,
            "scale {} != fit {}",
            viewport.scale,
            expected.scale
        );
        assert!(expected.scale < 1.0, "fixture should not fit at 100%");
    }

    #[gpui::test]
    fn actual_size_and_fit_actions(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx);
        let canvas = window.root(&mut cx).unwrap();
        canvas.update(&mut cx, |canvas, cx| {
            canvas
                .app_state()
                .update(cx, |state, cx| state.set_image(large_image(), cx));
        });
        cx.update(|window, cx| window.dispatch_action(Box::new(ActualSize), cx));
        let scale = canvas.read_with(&cx, |canvas, _| canvas.viewport().scale);
        assert!((scale - 1.0).abs() < 1e-4, "actual size scale {scale}");

        cx.update(|window, cx| window.dispatch_action(Box::new(Fit), cx));
        let viewport = canvas.read_with(&cx, |canvas, _| canvas.viewport());
        let expected = Viewport::fit(viewport.image_size, viewport.view_size);
        assert!((viewport.scale - expected.scale).abs() < 1e-3);
    }

    #[gpui::test]
    fn wheel_zoom_keeps_the_point_under_the_cursor(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx);
        let canvas = window.root(&mut cx).unwrap();
        canvas.update(&mut cx, |canvas, cx| {
            canvas
                .app_state()
                .update(cx, |state, cx| state.set_image(large_image(), cx));
        });
        let cursor = canvas.read_with(&cx, |canvas, _| {
            let bounds = canvas.content_bounds();
            point(
                bounds.origin.x + px(bounds.size.width.as_f32() / 2.0),
                bounds.origin.y + px(bounds.size.height.as_f32() / 2.0),
            )
        });
        let before = canvas.read_with(&cx, |canvas, _| {
            let bounds = canvas.content_bounds();
            let local = Point {
                x: (cursor.x - bounds.origin.x).as_f32(),
                y: (cursor.y - bounds.origin.y).as_f32(),
            };
            canvas.viewport().view_to_image(local)
        });
        cx.simulate_mouse_move(cursor, None, Default::default());
        cx.simulate_event(ScrollWheelEvent {
            position: cursor,
            delta: ScrollDelta::Lines(point(0.0, -1.0)),
            ..Default::default()
        });
        let after = canvas.read_with(&cx, |canvas, _| {
            let bounds = canvas.content_bounds();
            let local = Point {
                x: (cursor.x - bounds.origin.x).as_f32(),
                y: (cursor.y - bounds.origin.y).as_f32(),
            };
            canvas.viewport().view_to_image(local)
        });
        assert!(
            (before.x - after.x).abs() < 1e-2 && (before.y - after.y).abs() < 1e-2,
            "cursor image point moved from {before:?} to {after:?}"
        );
    }
}
