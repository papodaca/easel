mod viewport;

#[cfg(test)]
mod box_tool;

use easel_core::geometry::{Handle, Point, Rect, Size, ViewRect, Viewport, handle_at, hit_test};
use easel_core::scene::{BoxId, BoxKind};
use gpui::{
    App, Bounds, Context, CursorStyle, FocusHandle, Focusable, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render, ScrollDelta,
    ScrollWheelEvent, Styled, Window, canvas, div, img, prelude::*, px, rgb,
};

use crate::actions::{ActualSize, BoxTool, DeleteBox, Fit, SelectTool};
use crate::app_state::{AppState, Tool};
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
    virtual_frame: Option<Size>,
    drag: Drag,
}

enum Drag {
    Idle,
    Drawing {
        start: Point,
    },
    Moving {
        id: BoxId,
        origin: Rect,
        grab: Point,
    },
    Resizing {
        id: BoxId,
        handle: Handle,
    },
    DrawingTarget {
        start: Point,
    },
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
            virtual_frame: None,
            drag: Drag::Idle,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn shows_hint(&self, cx: &App) -> bool {
        self.app_state.read(cx).image.is_none() && self.app_state.read(cx).scene.frame().is_none()
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

    fn sync_frame(&mut self, cx: &mut Context<Self>) {
        let image = self.app_state.read(cx).image.clone();
        self.sync_image(image.as_deref());
        if image.is_some() {
            self.virtual_frame = None;
            return;
        }
        let frame = self.app_state.read(cx).scene.frame();
        if frame != self.virtual_frame {
            self.virtual_frame = frame;
            if let Some(frame) = frame {
                self.user_zoomed = false;
                self.viewport = Viewport::fit(frame, self.viewport.view_size);
            }
        }
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

    fn local_point(&self, position: gpui::Point<Pixels>) -> Point {
        Point {
            x: (position.x - self.bounds.origin.x).as_f32(),
            y: (position.y - self.bounds.origin.y).as_f32(),
        }
    }

    fn box_views(&self, cx: &App) -> Vec<(BoxId, Rect, ViewRect)> {
        let scene = &self.app_state.read(cx).scene;
        scene
            .boxes
            .iter()
            .filter_map(|element| {
                let rect = element.src.or(element.tgt)?;
                Some((element.id.clone(), rect, view_of(&self.viewport, rect)))
            })
            .collect()
    }

    fn on_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left || self.space_held {
            return;
        }
        let local = self.local_point(event.position);
        let image = self.viewport.view_to_image(local);
        let drawing_target = self.app_state.read(cx).draw_target.clone();
        if drawing_target.is_some() {
            self.drag = Drag::DrawingTarget { start: image };
            return;
        }
        if self.app_state.read(cx).tool == Tool::Box {
            if self.app_state.read(cx).box_tool_enabled() {
                self.drag = Drag::Drawing { start: image };
            }
            return;
        }
        let views = self.box_views(cx);
        if let Some(selected) = self.app_state.read(cx).selected_box.clone()
            && let Some((_, _, view)) = views.iter().find(|(id, _, _)| id == &selected)
            && let Some(handle) = handle_at(local, *view)
        {
            self.drag = Drag::Resizing {
                id: selected,
                handle,
            };
            return;
        }
        let rects: Vec<ViewRect> = views.iter().map(|(_, _, view)| *view).collect();
        if let Some(index) = hit_test(local, &rects) {
            let (id, rect, _) = views[index].clone();
            self.app_state.update(cx, |app, cx| {
                app.selected_box = Some(id.clone());
                cx.notify();
            });
            self.drag = Drag::Moving {
                id,
                origin: rect,
                grab: image,
            };
        } else {
            self.app_state.update(cx, |app, cx| {
                app.selected_box = None;
                cx.notify();
            });
        }
    }

    fn on_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !matches!(self.drag, Drag::Idle) {
            let image = self
                .viewport
                .view_to_image(self.local_point(event.position));
            match &self.drag {
                Drag::Moving { id, origin, grab } => {
                    let dx = (image.x - grab.x).round() as i32;
                    let dy = (image.y - grab.y).round() as i32;
                    let moved = Rect {
                        x: origin.x + dx,
                        y: origin.y + dy,
                        w: origin.w,
                        h: origin.h,
                    };
                    let id = id.clone();
                    self.app_state.update(cx, |app, cx| {
                        place_rect(&mut app.scene, &id, moved);
                        cx.notify();
                    });
                }
                Drag::Resizing { id, handle } => {
                    let id = id.clone();
                    let handle = *handle;
                    self.app_state.update(cx, |app, cx| {
                        if let Some(rect) = app
                            .scene
                            .boxes
                            .iter()
                            .find(|element| element.id == id)
                            .and_then(|element| element.src.or(element.tgt))
                        {
                            let resized = easel_core::geometry::resize_rect(rect, handle, image);
                            place_rect(&mut app.scene, &id, resized);
                            cx.notify();
                        }
                    });
                }
                _ => {}
            }
            cx.notify();
            return;
        }
        self.pan_move(event, cx);
    }

    fn on_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        let image = self
            .viewport
            .view_to_image(self.local_point(event.position));
        let drag = std::mem::replace(&mut self.drag, Drag::Idle);
        match drag {
            Drag::Drawing { start } => self.finish_box(start, image, false, cx),
            Drag::DrawingTarget { start } => self.finish_box(start, image, true, cx),
            _ => {}
        }
    }

    fn finish_box(&mut self, start: Point, end: Point, target: bool, cx: &mut Context<Self>) {
        let frame = self.app_state.read(cx).scene.frame();
        let rect = rect_between(start, end);
        let rect = frame.map(|frame| rect.clamp_to(frame)).unwrap_or(rect);
        if rect.w < 2 || rect.h < 2 {
            return;
        }
        self.app_state.update(cx, |app, cx| {
            if target {
                if let Some(id) = app.draw_target.clone() {
                    app.scene
                        .update_box(&id, |element| element.tgt = Some(rect));
                    app.draw_target = None;
                    app.revert_kind = None;
                }
            } else {
                let kind = if app.image.is_some() {
                    BoxKind::New
                } else {
                    BoxKind::Layout
                };
                let id = app.scene.add_box(rect, kind);
                app.selected_box = Some(id);
                app.focus_box_field = true;
            }
            cx.notify();
        });
    }

    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        self.app_state.update(cx, |app, cx| {
            if let Some(id) = app.selected_box.take() {
                app.scene.remove_box(&id);
                app.draw_target = None;
                cx.notify();
            }
        });
    }

    fn cancel_target(&mut self, cx: &mut Context<Self>) {
        self.drag = Drag::Idle;
        self.app_state.update(cx, |app, cx| {
            let Some(id) = app.draw_target.take() else {
                return;
            };
            let kind = app.revert_kind.take().unwrap_or(BoxKind::New);
            let src = app.saved_src.take();
            let tgt = app.saved_tgt.take();
            app.scene.update_box(&id, |element| {
                element.kind = kind;
                element.src = src;
                element.tgt = tgt;
            });
            cx.notify();
        });
    }
}

fn view_of(viewport: &Viewport, rect: Rect) -> ViewRect {
    let origin = viewport.image_to_view(Point {
        x: rect.x as f32,
        y: rect.y as f32,
    });
    let far = viewport.image_to_view(Point {
        x: (rect.x + rect.w) as f32,
        y: (rect.y + rect.h) as f32,
    });
    ViewRect {
        x: origin.x,
        y: origin.y,
        w: far.x - origin.x,
        h: far.y - origin.y,
    }
}

fn rect_between(start: Point, end: Point) -> Rect {
    Rect {
        x: start.x.round() as i32,
        y: start.y.round() as i32,
        w: (end.x - start.x).round() as i32,
        h: (end.y - start.y).round() as i32,
    }
    .normalize()
}

fn place_rect(scene: &mut easel_core::scene::Scene, id: &BoxId, rect: Rect) {
    scene.update_box(id, |element| match element.kind {
        BoxKind::Layout | BoxKind::Remove => element.src = Some(rect),
        BoxKind::New => element.tgt = Some(rect),
        BoxKind::Keep => {
            element.src = Some(rect);
            element.tgt = Some(rect);
        }
        BoxKind::Move => element.src = Some(rect),
    });
}

fn kind_color(kind: BoxKind) -> u32 {
    match kind {
        BoxKind::Layout => theme::BOX_LAYOUT,
        BoxKind::New => theme::BOX_NEW,
        BoxKind::Keep => theme::BOX_KEEP,
        BoxKind::Move => theme::BOX_MOVE,
        BoxKind::Remove => theme::BOX_REMOVE,
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
        self.sync_frame(cx);
        let image = self.app_state.read(cx).image.clone();
        let frame = self.app_state.read(cx).scene.frame();
        let boxes = self.app_state.read(cx).scene.boxes.clone();
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
            None if frame.is_some() => {
                let rect = self.viewport.image_rect_in_view();
                div()
                    .absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.w.max(0.0)))
                    .h(px(rect.h.max(0.0)))
                    .border_1()
                    .border_color(rgb(theme::BORDER))
                    .into_any_element()
            }
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .italic()
                .text_color(rgb(theme::TEXT_MUTED))
                .child("Open, drop, or paste an image, or pick an aspect ratio to draw boxes")
                .into_any_element(),
        };
        let overlays = boxes.into_iter().filter_map(|element| {
            let rect = element.src.or(element.tgt)?;
            let view = view_of(&self.viewport, rect);
            Some(
                div()
                    .absolute()
                    .left(px(view.x))
                    .top(px(view.y))
                    .w(px(view.w.max(0.0)))
                    .h(px(view.h.max(0.0)))
                    .border_1()
                    .border_color(rgb(kind_color(element.kind)))
                    .child(
                        div()
                            .absolute()
                            .top(px(-16.))
                            .text_xs()
                            .text_color(rgb(kind_color(element.kind)))
                            .child(element.id.as_str().to_string()),
                    ),
            )
        });
        let bounds_entity = cx.entity();
        div()
            .id("canvas")
            .key_context("Canvas")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Fit, _, cx| this.fit(cx)))
            .on_action(cx.listener(|this, _: &ActualSize, _, cx| this.actual_size(cx)))
            .on_action(cx.listener(|this, _: &SelectTool, _, cx| {
                this.app_state.update(cx, |app, cx| {
                    app.tool = Tool::Select;
                    cx.notify();
                });
            }))
            .on_action(cx.listener(|this, _: &BoxTool, _, cx| {
                this.app_state.update(cx, |app, cx| {
                    if app.box_tool_enabled() {
                        app.tool = Tool::Box;
                    }
                    cx.notify();
                });
            }))
            .on_action(cx.listener(|this, _: &DeleteBox, _, cx| this.delete_selected(cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "space" {
                    this.space_held = true;
                }
                if event.keystroke.key == "escape" {
                    this.cancel_target(cx);
                }
            }))
            .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, _, _| {
                if event.keystroke.key == "space" {
                    this.space_held = false;
                }
            }))
            .relative()
            .flex_1()
            .size_full()
            .min_h(px(0.))
            .overflow_hidden()
            .bg(rgb(theme::EDITOR))
            .text_color(rgb(theme::TEXT))
            .child(image_layer)
            .children(overlays)
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.on_down(event, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        this.on_drag(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseUpEvent, _, cx| {
                            this.on_up(event, cx);
                        }),
                    ),
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
