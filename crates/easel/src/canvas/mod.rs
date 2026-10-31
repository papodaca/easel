mod viewport;

#[cfg(test)]
mod box_tool;

use easel_core::geometry::{
    Handle, Point, Rect, Size, ViewRect, Viewport, edge_at, handle_at, hit_test,
};
use easel_core::scene::{BoxId, BoxKind};
use easel_core::sheet::{self, SheetLayout};
use gpui::{
    App, BorderStyle, Bounds, Context, CursorStyle, FocusHandle, Focusable, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, PathBuilder, Pixels,
    Render, ScrollDelta, ScrollEvent, SharedString, Styled, TextAlign, TextRun, Window, canvas,
    div, font, point, prelude::*, px, quad, rems, rgb, size,
};

use crate::actions::{ActualSize, BoxTool, DeleteBox, Fit, SelectTool};
use crate::app_state::{AppState, Tool};
use crate::notifications::Notifications;
use crate::theme;
use crate::ui::Button;

pub struct Canvas {
    focus_handle: FocusHandle,
    app_state: gpui::Entity<AppState>,
    notifications: gpui::Entity<Notifications>,
    viewport: Viewport,
    bounds: Bounds<Pixels>,
    user_zoomed: bool,
    space_held: bool,
    last_cursor: Option<gpui::Point<Pixels>>,
    pointer: CursorStyle,
    layout: SheetLayout,
    sizes: Vec<Size>,
    sheet_key: String,
    selected_tile: Option<usize>,
    drag: Drag,
}

struct PointerHit {
    slot: Option<usize>,
    pixel: Point,
    on_sheet: bool,
}

enum Drag {
    Idle,
    /// Press landed on a box. A short release selects it. A drag in the box tool starts a new box.
    Pressed {
        start_view: Point,
        slot: Option<usize>,
        start: Point,
    },
    Drawing {
        slot: Option<usize>,
        start: Point,
        current: Point,
    },
    Moving {
        id: BoxId,
        origin: Rect,
        grab: Point,
        slot: Option<usize>,
    },
    Resizing {
        id: BoxId,
        handle: Handle,
        slot: Option<usize>,
    },
    DrawingTarget {
        slot: Option<usize>,
        start: Point,
        current: Point,
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
            pointer: CursorStyle::Crosshair,
            layout: SheetLayout {
                content: Size { w: 0, h: 0 },
                tiles: Vec::new(),
            },
            sizes: Vec::new(),
            sheet_key: String::new(),
            selected_tile: None,
            drag: Drag::Idle,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn shows_hint(&self, cx: &App) -> bool {
        self.hint_text(cx).is_some()
    }

    pub fn hint_text(&self, cx: &App) -> Option<String> {
        let app = self.app_state.read(cx);
        if app.image.is_some() || app.scene.frame().is_some() {
            return None;
        }
        let mut text =
            "Open, drop, or paste an image, or pick an aspect ratio to draw boxes".to_string();
        if allows_empty_image_list(app) {
            text.push_str(". A prompt can run with no image");
        }
        Some(text)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn tile_count(&self) -> usize {
        self.layout.tiles.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn tile_view(&self, index: usize) -> Option<ViewRect> {
        self.layout
            .tiles
            .get(index)
            .map(|tile| content_to_view(&self.viewport, *tile))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn tile_label(&self, index: usize, cx: &App) -> Option<String> {
        self.app_state
            .read(cx)
            .reference_marks()
            .get(index)
            .map(|(label, _)| label.clone())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn tile_border(&self, index: usize) -> Option<u32> {
        if index >= self.layout.tiles.len() {
            return None;
        }
        Some(if self.selected_tile == Some(index) {
            theme::ACCENT
        } else {
            theme::BORDER
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn selected_tile(&self) -> Option<usize> {
        self.selected_tile
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn box_mark_view(&self, index: usize, cx: &App) -> Option<ViewRect> {
        let frame = self.inspection_frame(cx);
        let app = self.app_state.read(cx);
        let boxes = match &app.inspection {
            Some(inspection) => &inspection.boxes,
            None => &app.scene.boxes,
        };
        let element = boxes.get(index)?;
        let rect = element.src.or(element.tgt)?;
        Some(rect_view(
            &self.layout,
            &self.sizes,
            &self.viewport,
            element.from_image,
            rect,
            frame,
        ))
    }

    #[cfg(test)]
    pub fn nudge(&mut self, dx: f32, dy: f32) {
        self.viewport.pan_by(Point { x: dx, y: dy });
        self.user_zoomed = true;
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
        self.sync_sheet(cx);
        self.user_zoomed = false;
        self.refit(cx);
        cx.notify();
    }

    pub fn actual_size(&mut self, cx: &mut Context<Self>) {
        self.user_zoomed = true;
        self.viewport.actual_size();
        cx.notify();
    }

    fn refit(&mut self, _cx: &mut Context<Self>) {
        if self.layout.content.w == 0 && self.layout.content.h == 0 {
            return;
        }
        self.viewport = Viewport::fit(self.layout.content, self.viewport.view_size);
    }

    fn note_bounds(&mut self, bounds: Bounds<Pixels>) -> (Viewport, bool) {
        let view_size =
            viewport::size_from_pixels(bounds.size.width.as_f32(), bounds.size.height.as_f32());
        if self.bounds == bounds && self.viewport.view_size == view_size {
            return (self.viewport, false);
        }
        self.bounds = bounds;
        self.viewport.view_size = view_size;
        if self.user_zoomed {
            self.viewport.clamp();
        } else if self.viewport.image_size.w > 0 || self.viewport.image_size.h > 0 {
            let image_size = self.viewport.image_size;
            self.viewport = Viewport::fit(image_size, view_size);
        }
        (self.viewport, true)
    }

    fn sync_sheet(&mut self, cx: &mut Context<Self>) {
        let (sizes, aspect, key) = {
            let app = self.app_state.read(cx);
            let mut sizes = Vec::new();
            let mut key = String::new();
            if let Some(image) = &app.image {
                sizes.push(image.size);
                key.push_str(&format!(
                    "s:{}:{}:{},",
                    image.render.id.0, image.size.w, image.size.h
                ));
            }
            for extra in &app.extras {
                sizes.push(extra.size);
                key.push_str(&format!(
                    "e:{}:{}:{},",
                    extra.render.id.0, extra.size.w, extra.size.h
                ));
            }
            let aspect = app.scene.aspect_ratio.clone();
            key.push_str(aspect.as_deref().unwrap_or(""));
            (sizes, aspect, key)
        };
        if key == self.sheet_key {
            return;
        }
        self.sheet_key = key;
        self.sizes = sizes;
        self.layout = sheet::layout(&self.sizes, aspect.as_deref());
        if let Some(index) = self.selected_tile
            && index >= self.layout.tiles.len()
        {
            self.selected_tile = self.layout.tiles.len().checked_sub(1);
        }
        self.user_zoomed = false;
        if self.layout.content.w > 0 || self.layout.content.h > 0 {
            self.viewport = Viewport::fit(self.layout.content, self.viewport.view_size);
        } else {
            self.viewport.image_size = Size { w: 0, h: 0 };
        }
    }

    fn zoom_wheel(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let local = Point {
            x: (event.position.x - self.bounds.origin.x).as_f32(),
            y: (event.position.y - self.bounds.origin.y).as_f32(),
        };
        self.viewport
            .zoom_at(local, scroll_zoom_factor(event.delta));
        self.user_zoomed = true;
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn pointer(&self) -> CursorStyle {
        self.pointer
    }

    fn note_pointer(&mut self, local: Point, cx: &mut Context<Self>) {
        let next = match self.drag {
            Drag::Resizing { handle, .. } => cursor_for(handle),
            Drag::Idle => self.hover_cursor(local, cx),
            _ => CursorStyle::Crosshair,
        };
        if next != self.pointer {
            self.pointer = next;
            cx.notify();
        }
    }

    fn hover_cursor(&self, local: Point, cx: &App) -> CursorStyle {
        let Some(selected) = self.app_state.read(cx).selected_box.clone() else {
            return CursorStyle::Crosshair;
        };
        let Some((_, _, view)) = self
            .box_views(cx)
            .into_iter()
            .find(|(id, _, _)| id == &selected)
        else {
            return CursorStyle::Crosshair;
        };
        handle_at(local, view)
            .map(cursor_for)
            .unwrap_or(CursorStyle::Crosshair)
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
                Some((
                    element.id.clone(),
                    rect,
                    rect_view(
                        &self.layout,
                        &self.sizes,
                        &self.viewport,
                        element.from_image,
                        rect,
                        self.inspection_frame(cx),
                    ),
                ))
            })
            .collect()
    }

    fn inspection_frame(&self, cx: &App) -> Option<Size> {
        let inspection = self.app_state.read(cx).inspection.as_ref()?;
        Some(inspection.frame.unwrap_or(Size { w: 1000, h: 1000 }))
    }

    fn pointer_hit(&self, local: Point) -> PointerHit {
        let content = self.viewport.view_to_image(local);
        if let Some(hit) = sheet::hit_tile(&self.layout, &self.sizes, content) {
            return PointerHit {
                slot: Some(hit.index),
                pixel: hit.pixel,
                on_sheet: true,
            };
        }
        if self.layout.tiles.is_empty() && (self.layout.content.w > 0 || self.layout.content.h > 0)
        {
            let size = self.layout.content;
            let inside = content.x >= 0.0
                && content.y >= 0.0
                && content.x <= size.w as f32
                && content.y <= size.h as f32;
            return PointerHit {
                slot: None,
                pixel: Point {
                    x: content.x.clamp(0.0, size.w as f32),
                    y: content.y.clamp(0.0, size.h as f32),
                },
                on_sheet: inside,
            };
        }
        PointerHit {
            slot: None,
            pixel: content,
            on_sheet: false,
        }
    }

    fn clamped_pixel(&self, local: Point, slot: Option<usize>) -> Point {
        let content = self.viewport.view_to_image(local);
        if let Some(slot) = slot
            && let Some(pixel) = sheet::clamp_tile(&self.layout, &self.sizes, slot, content)
        {
            return pixel;
        }
        if self.layout.tiles.is_empty() {
            let size = self.layout.content;
            return Point {
                x: content.x.clamp(0.0, size.w as f32),
                y: content.y.clamp(0.0, size.h as f32),
            };
        }
        content
    }

    fn draft_rect(&self) -> Option<Rect> {
        match self.drag {
            Drag::Drawing { start, current, .. } | Drag::DrawingTarget { start, current, .. } => {
                Some(rect_between(start, current))
            }
            _ => None,
        }
    }

    fn on_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left || self.space_held {
            return;
        }
        let local = self.local_point(event.position);
        let hit = self.pointer_hit(local);
        if self.app_state.read(cx).draw_target.is_some() {
            let slot = self.draw_target_slot(cx);
            let pixel = self.clamped_pixel(local, slot);
            self.drag = Drag::DrawingTarget {
                slot,
                start: pixel,
                current: pixel,
            };
            return;
        }
        let views = self.box_views(cx);
        if event.modifiers.alt
            && let Some(index) = hit_test(
                local,
                &views.iter().map(|(_, _, view)| *view).collect::<Vec<_>>(),
            )
        {
            let (id, rect, _) = views[index].clone();
            let slot = self.slot_of(&id, cx);
            self.select_box(id.clone(), cx);
            self.note_tile(slot);
            self.drag = Drag::Moving {
                id,
                origin: rect,
                grab: self.clamped_pixel(local, slot),
                slot,
            };
            return;
        }
        let selected = self.app_state.read(cx).selected_box.clone();
        if let Some((id, handle)) = Self::edge_target(local, selected.as_ref(), &views) {
            let slot = self.slot_of(&id, cx);
            self.select_box(id.clone(), cx);
            self.note_tile(slot);
            self.drag = Drag::Resizing { id, handle, slot };
            return;
        }
        let rects: Vec<ViewRect> = views.iter().map(|(_, _, view)| *view).collect();
        if let Some(index) = hit_test(local, &rects) {
            let (id, _, _) = views[index].clone();
            let slot = self.slot_of(&id, cx).or(hit.slot);
            self.select_box(id, cx);
            self.note_tile(slot);
            self.drag = Drag::Pressed {
                start_view: local,
                slot,
                start: self.clamped_pixel(local, slot),
            };
            return;
        }
        if hit.on_sheet
            && self.app_state.read(cx).tool == Tool::Box
            && self.app_state.read(cx).box_tool_enabled()
        {
            self.note_tile(hit.slot);
            self.drag = Drag::Drawing {
                slot: hit.slot,
                start: hit.pixel,
                current: hit.pixel,
            };
            cx.notify();
            return;
        }
        if hit.slot.is_some() {
            self.note_tile(hit.slot);
        }
        self.app_state.update(cx, |app, cx| {
            app.selected_box = None;
            cx.notify();
        });
    }

    fn note_tile(&mut self, slot: Option<usize>) {
        if slot.is_some() {
            self.selected_tile = slot;
        }
    }

    fn slot_of(&self, id: &BoxId, cx: &App) -> Option<usize> {
        self.app_state
            .read(cx)
            .scene
            .boxes
            .iter()
            .find(|element| &element.id == id)
            .and_then(|element| element.from_image)
    }

    fn draw_target_slot(&self, cx: &App) -> Option<usize> {
        let id = self.app_state.read(cx).draw_target.clone()?;
        self.slot_of(&id, cx)
    }

    fn select_box(&mut self, id: BoxId, cx: &mut Context<Self>) {
        self.app_state.update(cx, |app, cx| {
            app.selected_box = Some(id);
            cx.notify();
        });
    }

    fn edge_target(
        local: Point,
        selected: Option<&BoxId>,
        views: &[(BoxId, Rect, ViewRect)],
    ) -> Option<(BoxId, Handle)> {
        if let Some(selected) = selected
            && let Some((id, _, view)) = views.iter().find(|(id, _, _)| id == selected)
            && let Some(handle) = edge_at(local, *view)
        {
            return Some((id.clone(), handle));
        }
        views
            .iter()
            .rev()
            .find_map(|(id, _, view)| edge_at(local, *view).map(|handle| (id.clone(), handle)))
    }

    fn on_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let local = self.local_point(event.position);
        self.note_pointer(local, cx);
        if matches!(self.drag, Drag::Idle) {
            self.pan_move(event, cx);
            return;
        }
        let drag = std::mem::replace(&mut self.drag, Drag::Idle);
        match drag {
            Drag::Pressed {
                start_view,
                slot,
                start,
            } => {
                let moved = (local.x - start_view.x).abs() + (local.y - start_view.y).abs();
                self.drag = if moved > 4.0
                    && self.app_state.read(cx).tool == Tool::Box
                    && self.app_state.read(cx).box_tool_enabled()
                {
                    Drag::Drawing {
                        slot,
                        start,
                        current: self.clamped_pixel(local, slot),
                    }
                } else {
                    Drag::Pressed {
                        start_view,
                        slot,
                        start,
                    }
                };
            }
            Drag::Drawing { slot, start, .. } => {
                self.drag = Drag::Drawing {
                    slot,
                    start,
                    current: self.clamped_pixel(local, slot),
                };
            }
            Drag::DrawingTarget { slot, start, .. } => {
                self.drag = Drag::DrawingTarget {
                    slot,
                    start,
                    current: self.clamped_pixel(local, slot),
                };
            }
            Drag::Moving {
                id,
                origin,
                grab,
                slot,
            } => {
                let image = self.clamped_pixel(local, slot);
                let dx = (image.x - grab.x).round() as i32;
                let dy = (image.y - grab.y).round() as i32;
                let moved = Rect {
                    x: origin.x + dx,
                    y: origin.y + dy,
                    w: origin.w,
                    h: origin.h,
                };
                self.app_state.update(cx, |app, cx| {
                    place_rect(&mut app.scene, &id, moved);
                    cx.notify();
                });
                self.drag = Drag::Moving {
                    id,
                    origin,
                    grab,
                    slot,
                };
            }
            Drag::Resizing { id, handle, slot } => {
                let image = self.clamped_pixel(local, slot);
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
                self.drag = Drag::Resizing { id, handle, slot };
            }
            Drag::Idle => {}
        }
        cx.notify();
    }

    fn on_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        let local = self.local_point(event.position);
        let drag = std::mem::replace(&mut self.drag, Drag::Idle);
        match drag {
            Drag::Drawing { slot, start, .. } => {
                let end = self.clamped_pixel(local, slot);
                self.finish_box(slot, start, end, false, cx);
            }
            Drag::DrawingTarget { slot, start, .. } => {
                let end = self.clamped_pixel(local, slot);
                self.finish_box(slot, start, end, true, cx);
            }
            _ => {}
        }
        self.note_pointer(self.local_point(event.position), cx);
    }

    fn finish_box(
        &mut self,
        slot: Option<usize>,
        start: Point,
        end: Point,
        target: bool,
        cx: &mut Context<Self>,
    ) {
        let limit = slot
            .and_then(|index| self.sizes.get(index).copied())
            .or_else(|| self.app_state.read(cx).scene.frame());
        let rect = rect_between(start, end);
        let rect = limit.map(|limit| rect.clamp_to(limit)).unwrap_or(rect);
        if rect.w < 2 || rect.h < 2 {
            self.note_tile(slot);
            if !target {
                self.app_state.update(cx, |app, cx| {
                    app.selected_box = None;
                    cx.notify();
                });
            }
            return;
        }
        self.note_tile(slot);
        self.app_state.update(cx, |app, cx| {
            if target {
                if let Some(id) = app.draw_target.clone() {
                    app.scene
                        .update_box(&id, |element| element.tgt = Some(rect));
                    app.draw_target = None;
                    app.revert_kind = None;
                }
            } else {
                let kind = if slot.is_some() || app.image.is_some() {
                    BoxKind::New
                } else {
                    BoxKind::Layout
                };
                let id = if let Some(slot) = slot {
                    app.scene.add_box_on(rect, kind, slot)
                } else {
                    app.scene.add_box(rect, kind)
                };
                app.selected_box = Some(id);
                app.focus_box_field = true;
            }
            cx.notify();
        });
    }

    fn tile_overlays(&self, cx: &App) -> gpui::Div {
        let marks = self.app_state.read(cx).reference_marks();
        let mut overlays = div();
        for (index, tile) in self.layout.tiles.iter().enumerate() {
            let view = content_to_view(&self.viewport, *tile);
            let label = marks
                .get(index)
                .map(|(label, _)| label.clone())
                .unwrap_or_default();
            let border = if self.selected_tile == Some(index) {
                theme::ACCENT
            } else {
                theme::BORDER
            };
            overlays = overlays.child(
                div()
                    .absolute()
                    .left(px(view.x))
                    .top(px(view.y))
                    .w(px(view.w.max(0.0)))
                    .h(px(view.h.max(0.0)))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(border))
                    .child(
                        div()
                            .absolute()
                            .top(px(4.))
                            .left(px(4.))
                            .right(px(4.))
                            .overflow_hidden()
                            .text_xs()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child(label),
                    ),
            );
        }
        overlays
    }

    fn tile_actions(&self, cx: &mut Context<Self>) -> gpui::Div {
        let Some(index) = self.selected_tile else {
            return div();
        };
        let Some(view) = self.tile_view(index) else {
            return div();
        };
        let names = action_names(index, self.layout.tiles.len(), view.w);
        let entity = cx.entity();
        let mut row = div()
            .absolute()
            .left(px(view.x))
            .top(px(view.y))
            .w(px(view.w.max(0.0)))
            .h(px(view.h.max(0.0)))
            .overflow_hidden()
            .flex()
            .items_end()
            .gap(px(4.))
            .p(px(4.));
        for name in names {
            let entity = entity.clone();
            row = row.child(
                Button::ghost(SharedString::from(format!("tile-{name}-{index}")), name).on_click(
                    move |_, _, cx| {
                        entity.update(cx, |canvas, cx| canvas.apply_tile_action(index, name, cx));
                    },
                ),
            );
        }
        row
    }

    fn sheet_note(&self, cx: &App) -> gpui::Div {
        let Some(note) = self.app_state.read(cx).sheet_note.clone() else {
            return div();
        };
        let anchor = self
            .tile_view(0)
            .unwrap_or_else(|| self.viewport.image_rect_in_view());
        div()
            .absolute()
            .left(px(anchor.x))
            .top(px(anchor.y + anchor.h + 4.0))
            .text_xs()
            .text_color(rgb(theme::TEXT_MUTED))
            .child(note)
    }

    fn apply_tile_action(&mut self, index: usize, name: &str, cx: &mut Context<Self>) {
        match name {
            "Remove" => {
                self.app_state
                    .update(cx, |app, cx| app.remove_reference_slot(index, cx));
                let count = {
                    let app = self.app_state.read(cx);
                    usize::from(app.image.is_some()) + app.extras.len()
                };
                self.selected_tile = if count == 0 {
                    None
                } else if index >= count {
                    Some(count - 1)
                } else {
                    Some(index)
                };
            }
            "Move left" if index > 0 => self.move_tile(index, index - 1, cx),
            "Move right" => self.move_tile(index, index + 1, cx),
            _ => {}
        }
        cx.notify();
    }

    fn move_tile(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.app_state
            .update(cx, |app, cx| app.move_slot(from, to, cx));
        self.selected_tile = Some(to);
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

fn paint_marks(
    origin: gpui::Point<Pixels>,
    layout: &SheetLayout,
    sizes: &[Size],
    viewport: &Viewport,
    boxes: &[easel_core::scene::BoxElement],
    opacity: f32,
    forced: Option<Size>,
    window: &mut Window,
) {
    for element in boxes {
        let packed = kind_color(element.kind);
        let fill = theme::color(packed, theme::BOX_FILL_ALPHA * opacity);
        let outline = theme::color(packed, opacity);
        if element.kind == BoxKind::Move {
            if let Some(rect) = element.src {
                paint_mark(
                    origin,
                    layout,
                    sizes,
                    viewport,
                    element.from_image,
                    rect,
                    fill,
                    outline,
                    BorderStyle::Dashed,
                    forced,
                    window,
                );
            }
            if let Some(rect) = element.tgt {
                paint_mark(
                    origin,
                    layout,
                    sizes,
                    viewport,
                    element.from_image,
                    rect,
                    fill,
                    outline,
                    BorderStyle::Solid,
                    forced,
                    window,
                );
            }
            if let (Some(src), Some(tgt)) = (element.src, element.tgt) {
                paint_arrow(
                    origin,
                    layout,
                    sizes,
                    viewport,
                    element.from_image,
                    src,
                    tgt,
                    outline,
                    forced,
                    window,
                );
            }
        } else if let Some(rect) = element.src.or(element.tgt) {
            paint_mark(
                origin,
                layout,
                sizes,
                viewport,
                element.from_image,
                rect,
                fill,
                outline,
                BorderStyle::Solid,
                forced,
                window,
            );
        }
    }
}

fn cursor_for(handle: Handle) -> CursorStyle {
    match handle {
        Handle::North | Handle::South => CursorStyle::ResizeUpDown,
        Handle::East | Handle::West => CursorStyle::ResizeLeftRight,
        Handle::NorthWest | Handle::SouthEast => CursorStyle::ResizeUpLeftDownRight,
        Handle::NorthEast | Handle::SouthWest => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn label_rect(element: &easel_core::scene::BoxElement) -> Option<Rect> {
    if element.kind == BoxKind::Move {
        element.tgt.or(element.src)
    } else {
        element.src.or(element.tgt)
    }
}

fn paint_labels(
    origin: gpui::Point<Pixels>,
    layout: &SheetLayout,
    sizes: &[Size],
    viewport: &Viewport,
    boxes: &[easel_core::scene::BoxElement],
    forced: Option<Size>,
    window: &mut Window,
    cx: &mut App,
) {
    let font_size = rems(0.75).to_pixels(window.rem_size());
    let line_height = px(16.0);
    let face = font(crate::assets::UI_FONT);
    for element in boxes {
        let Some(rect) = label_rect(element) else {
            continue;
        };
        let view = rect_view(layout, sizes, viewport, element.from_image, rect, forced);
        let text = SharedString::from(element.id.as_str().to_string());
        let run = TextRun {
            len: text.len(),
            font: face.clone(),
            color: theme::color(kind_color(element.kind), 1.0).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped =
            window
                .text_system()
                .shape_line(text, font_size, std::slice::from_ref(&run), None);
        let _ = shaped.paint(
            point(origin.x + px(view.x), origin.y + px(view.y) - line_height),
            line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

fn paint_handles(
    origin: gpui::Point<Pixels>,
    layout: &SheetLayout,
    sizes: &[Size],
    viewport: &Viewport,
    slot: Option<usize>,
    rect: Rect,
    forced: Option<Size>,
    window: &mut Window,
) {
    let view = rect_view(layout, sizes, viewport, slot, rect, forced);
    let anchors = [
        (view.x, view.y),
        (view.x + view.w / 2.0, view.y),
        (view.x + view.w, view.y),
        (view.x + view.w, view.y + view.h / 2.0),
        (view.x + view.w, view.y + view.h),
        (view.x + view.w / 2.0, view.y + view.h),
        (view.x, view.y + view.h),
        (view.x, view.y + view.h / 2.0),
    ];
    let fill = theme::color(theme::TEXT, 1.0);
    for (x, y) in anchors {
        window.paint_quad(quad(
            Bounds {
                origin: point(origin.x + px(x - 3.0), origin.y + px(y - 3.0)),
                size: size(px(6.0), px(6.0)),
            },
            0.0,
            fill,
            0.0,
            fill,
            BorderStyle::Solid,
        ));
    }
}

fn paint_mark(
    origin: gpui::Point<Pixels>,
    layout: &SheetLayout,
    sizes: &[Size],
    viewport: &Viewport,
    slot: Option<usize>,
    rect: Rect,
    fill: gpui::Rgba,
    outline: gpui::Rgba,
    style: BorderStyle,
    forced: Option<Size>,
    window: &mut Window,
) {
    let view = rect_view(layout, sizes, viewport, slot, rect, forced);
    window.paint_quad(quad(
        Bounds {
            origin: point(origin.x + px(view.x), origin.y + px(view.y)),
            size: size(px(view.w.max(0.0)), px(view.h.max(0.0))),
        },
        0.0,
        fill,
        theme::BOX_OUTLINE_WIDTH,
        outline,
        style,
    ));
}

fn paint_arrow(
    origin: gpui::Point<Pixels>,
    layout: &SheetLayout,
    sizes: &[Size],
    viewport: &Viewport,
    slot: Option<usize>,
    src: Rect,
    tgt: Rect,
    color: gpui::Rgba,
    forced: Option<Size>,
    window: &mut Window,
) {
    let src = rect_view(layout, sizes, viewport, slot, src, forced);
    let tgt = rect_view(layout, sizes, viewport, slot, tgt, forced);
    let from = point(
        origin.x + px(src.x + src.w / 2.0),
        origin.y + px(src.y + src.h / 2.0),
    );
    let to = point(
        origin.x + px(tgt.x + tgt.w / 2.0),
        origin.y + px(tgt.y + tgt.h / 2.0),
    );
    let mut path = PathBuilder::stroke(px(theme::BOX_OUTLINE_WIDTH));
    path.move_to(from);
    path.line_to(to);
    let dx = (to.x - from.x).as_f32();
    let dy = (to.y - from.y).as_f32();
    let len = (dx * dx + dy * dy).sqrt();
    if len > 1.0 {
        let ux = dx / len;
        let uy = dy / len;
        let head = 12.0;
        let wing = 5.0;
        let base_x = to.x.as_f32() - ux * head;
        let base_y = to.y.as_f32() - uy * head;
        path.move_to(to);
        path.line_to(point(px(base_x - uy * wing), px(base_y + ux * wing)));
        path.move_to(to);
        path.line_to(point(px(base_x + uy * wing), px(base_y - ux * wing)));
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn rect_view(
    layout: &SheetLayout,
    sizes: &[Size],
    viewport: &Viewport,
    slot: Option<usize>,
    rect: Rect,
    forced: Option<Size>,
) -> ViewRect {
    if let Some(frame) = forced.filter(|frame| frame.w > 0 && frame.h > 0) {
        let tile = layout.tiles.first().copied().unwrap_or(ViewRect {
            x: 0.0,
            y: 0.0,
            w: layout.content.w as f32,
            h: layout.content.h as f32,
        });
        return content_to_view(viewport, tile_local(tile, frame, rect));
    }
    let content = if let Some(index) = slot {
        match (layout.tiles.get(index), sizes.get(index)) {
            (Some(tile), Some(size)) if size.w > 0 && size.h > 0 => tile_local(*tile, *size, rect),
            _ => pixels_as_content(rect),
        }
    } else if layout.tiles.is_empty() {
        pixels_as_content(rect)
    } else if let (Some(tile), Some(size)) = (layout.tiles.first(), sizes.first()) {
        tile_local(*tile, *size, rect)
    } else {
        pixels_as_content(rect)
    };
    content_to_view(viewport, content)
}

fn tile_local(tile: ViewRect, size: Size, rect: Rect) -> ViewRect {
    ViewRect {
        x: tile.x + rect.x as f32 / size.w as f32 * tile.w,
        y: tile.y + rect.y as f32 / size.h as f32 * tile.h,
        w: rect.w as f32 / size.w as f32 * tile.w,
        h: rect.h as f32 / size.h as f32 * tile.h,
    }
}

fn pixels_as_content(rect: Rect) -> ViewRect {
    ViewRect {
        x: rect.x as f32,
        y: rect.y as f32,
        w: rect.w as f32,
        h: rect.h as f32,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn view_of(viewport: &Viewport, rect: Rect) -> ViewRect {
    content_to_view(viewport, pixels_as_content(rect))
}

fn content_to_view(viewport: &Viewport, rect: ViewRect) -> ViewRect {
    let origin = viewport.image_to_view(Point {
        x: rect.x,
        y: rect.y,
    });
    let far = viewport.image_to_view(Point {
        x: rect.x + rect.w,
        y: rect.y + rect.h,
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

fn allows_empty_image_list(app: &AppState) -> bool {
    app.field_errors(0)
        .iter()
        .all(|error| error.key != "images" && !error.message.to_ascii_lowercase().contains("image"))
}

fn action_names(index: usize, count: usize, width: f32) -> Vec<&'static str> {
    const GAP: f32 = 4.0;
    const REMOVE: f32 = 78.0;
    const MOVE_LEFT: f32 = 92.0;
    const MOVE_RIGHT: f32 = 100.0;
    let inner = (width - 8.0).max(0.0);
    let want_left = index > 0;
    let want_right = index + 1 < count;
    let mut all = REMOVE;
    if want_left {
        all += GAP + MOVE_LEFT;
    }
    if want_right {
        all += GAP + MOVE_RIGHT;
    }
    if inner >= all {
        let mut names = Vec::new();
        if want_left {
            names.push("Move left");
        }
        names.push("Remove");
        if want_right {
            names.push("Move right");
        }
        return names;
    }
    let mut names = vec!["Remove"];
    if want_left && inner >= REMOVE + GAP + MOVE_LEFT {
        names.insert(0, "Move left");
    } else if want_right && inner >= REMOVE + GAP + MOVE_RIGHT {
        names.push("Move right");
    }
    names
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
        self.sync_sheet(cx);
        let image = self.app_state.read(cx).image.clone();
        let frame = self.app_state.read(cx).scene.frame();
        let boxes = {
            let app = self.app_state.read(cx);
            match &app.inspection {
                Some(inspection) => inspection.boxes.clone(),
                None => app.scene.boxes.clone(),
            }
        };
        let tile_images = {
            let app = self.app_state.read(cx);
            let mut images = Vec::new();
            if let Some(image) = &app.image {
                images.push(image.render.clone());
            }
            for extra in &app.extras {
                images.push(extra.render.clone());
            }
            images
        };
        let hint = self.hint_text(cx);
        let image_layer = if let Some(hint) = hint {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .italic()
                .text_color(rgb(theme::TEXT_MUTED))
                .child(hint)
                .into_any_element()
        } else if image.is_none() && frame.is_some() {
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
        } else {
            div().into_any_element()
        };
        let painted = boxes.clone();
        let layout = self.layout.clone();
        let sizes = self.sizes.clone();
        let boxes_live = self.app_state.read(cx).spec().capabilities.supports_boxes;
        let inspect_frame = self.inspection_frame(cx);
        let draft_slot = match &self.drag {
            Drag::Drawing { slot, .. } | Drag::DrawingTarget { slot, .. } => *slot,
            _ => None,
        };
        let draft = self.draft_rect().map(|rect| {
            let limit = draft_slot
                .and_then(|index| self.sizes.get(index).copied())
                .or_else(|| self.app_state.read(cx).scene.frame());
            limit.map(|limit| rect.clamp_to(limit)).unwrap_or(rect)
        });
        let selected_mark = {
            let selected = self.app_state.read(cx).selected_box.clone();
            boxes.iter().find_map(|element| {
                if selected.as_ref() != Some(&element.id) {
                    return None;
                }
                element
                    .src
                    .or(element.tgt)
                    .map(|rect| (element.from_image, rect))
            })
        };
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
            .child(self.tile_overlays(cx))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        // A resize that arrives mid-draw (GNOME snap) cannot
                        // schedule a frame with `notify`. Measure here and paint
                        // this frame from the new size.
                        let (viewport, changed) =
                            bounds_entity.update(cx, |canvas, _| canvas.note_bounds(bounds));
                        if changed {
                            let again = bounds_entity.clone();
                            window.on_next_frame(move |_window, cx| {
                                again.update(cx, |_, cx| cx.notify());
                            });
                        }
                        (
                            viewport,
                            painted,
                            draft,
                            draft_slot,
                            selected_mark,
                            tile_images,
                            layout,
                            sizes,
                            boxes_live,
                            inspect_frame,
                        )
                    },
                    |bounds,
                     (
                        viewport,
                        painted,
                        draft,
                        draft_slot,
                        selected,
                        tile_images,
                        layout,
                        sizes,
                        boxes_live,
                        inspect_frame,
                    ),
                     window,
                     cx| {
                        for (index, render_image) in tile_images.iter().enumerate() {
                            let Some(tile) = layout.tiles.get(index) else {
                                continue;
                            };
                            let rect = content_to_view(&viewport, *tile);
                            let image_bounds = Bounds {
                                origin: point(
                                    bounds.origin.x + px(rect.x),
                                    bounds.origin.y + px(rect.y),
                                ),
                                size: size(px(rect.w.max(0.0)), px(rect.h.max(0.0))),
                            };
                            window
                                .paint_image(
                                    image_bounds,
                                    image_bounds,
                                    gpui::Corners::default(),
                                    render_image.clone(),
                                    0,
                                    false,
                                )
                                .ok();
                        }
                        paint_marks(
                            bounds.origin,
                            &layout,
                            &sizes,
                            &viewport,
                            &painted,
                            if boxes_live { 1.0 } else { 0.4 },
                            inspect_frame,
                            window,
                        );
                        paint_labels(
                            bounds.origin,
                            &layout,
                            &sizes,
                            &viewport,
                            &painted,
                            inspect_frame,
                            window,
                            cx,
                        );
                        if let Some(rect) = draft {
                            paint_mark(
                                bounds.origin,
                                &layout,
                                &sizes,
                                &viewport,
                                draft_slot,
                                rect,
                                theme::color(theme::BOX_NEW, theme::BOX_FILL_ALPHA),
                                theme::color(theme::BOX_NEW, 1.0),
                                BorderStyle::Solid,
                                inspect_frame,
                                window,
                            );
                        }
                        if boxes_live && let Some((slot, rect)) = selected {
                            paint_handles(
                                bounds.origin,
                                &layout,
                                &sizes,
                                &viewport,
                                slot,
                                rect,
                                inspect_frame,
                                window,
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id("canvas-hit")
                    .absolute()
                    .size_full()
                    .cursor(self.pointer)
                    .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
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
            .child(self.tile_actions(cx))
            .child(self.sheet_note(cx))
            .child(self.notifications.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use easel_core::geometry::{Point, Size, Viewport};
    use gpui::{
        Focusable as _, ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext, point,
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
        cx.simulate_event(ScrollEvent {
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

    #[gpui::test]
    fn compare_draws_saved_boxes_on_the_first_tile_only(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx);
        let canvas = window.root(&mut cx).unwrap();
        canvas.update(&mut cx, |canvas, cx| {
            canvas.app_state().update(cx, |app, cx| {
                let bytes = std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/pixel.png"
                ))
                .unwrap();
                let mut first = decode_image(&bytes, "a.png").unwrap();
                first.size = Size { w: 100, h: 100 };
                let mut second = decode_image(&bytes, "b.png").unwrap();
                second.size = Size { w: 400, h: 200 };
                app.set_image(Arc::new(first), cx);
                app.add_extra(Arc::new(second), cx).unwrap();
                app.scene.add_box(
                    easel_core::geometry::Rect {
                        x: 0,
                        y: 0,
                        w: 10,
                        h: 10,
                    },
                    easel_core::scene::BoxKind::Keep,
                );
                app.inspection = Some(crate::jobs::Inspection {
                    prompt: String::new(),
                    suggestion: String::new(),
                    rows: Vec::new(),
                    boxes: vec![easel_core::scene::BoxElement {
                        id: easel_core::scene::BoxId::parse("old_1").unwrap(),
                        kind: easel_core::scene::BoxKind::Keep,
                        desc: String::new(),
                        src: Some(easel_core::geometry::Rect {
                            x: 0,
                            y: 0,
                            w: 40,
                            h: 20,
                        }),
                        tgt: Some(easel_core::geometry::Rect {
                            x: 0,
                            y: 0,
                            w: 40,
                            h: 20,
                        }),
                        from_image: Some(1),
                    }],
                    frame: Some(Size { w: 80, h: 40 }),
                });
                cx.notify();
            });
        });
        let (tile0, tile1, mark) = canvas.read_with(&cx, |canvas, cx| {
            (
                canvas.tile_view(0).unwrap(),
                canvas.tile_view(1).unwrap(),
                canvas.box_mark_view(0, cx).unwrap(),
            )
        });
        assert!((mark.w - tile0.w / 2.0).abs() < 1.0, "{mark:?} {tile0:?}");
        assert!((mark.h - tile0.h / 2.0).abs() < 1.0, "{mark:?} {tile0:?}");
        assert!((mark.x - tile0.x).abs() < 1.0);
        assert!(mark.x + mark.w <= tile1.x + 1.0, "{mark:?} {tile1:?}");
        let before = tile1.y;
        canvas.update(&mut cx, |canvas, _| canvas.nudge(0.0, 30.0));
        let (tile0_after, tile1_after, mark_after) = canvas.read_with(&cx, |canvas, cx| {
            (
                canvas.tile_view(0).unwrap(),
                canvas.tile_view(1).unwrap(),
                canvas.box_mark_view(0, cx).unwrap(),
            )
        });
        assert!((tile0_after.y - (tile0.y + 30.0)).abs() < 1.0);
        assert!((tile1_after.y - (before + 30.0)).abs() < 1.0);
        assert!((mark_after.y - (mark.y + 30.0)).abs() < 1.0);
        canvas.update(&mut cx, |canvas, cx| {
            canvas.app_state().update(cx, |app, cx| {
                app.inspection = None;
                cx.notify();
            });
        });
        let live = canvas.read_with(&cx, |canvas, cx| canvas.box_mark_view(0, cx).unwrap());
        let tile0 = canvas.read_with(&cx, |canvas, _| canvas.tile_view(0).unwrap());
        assert!((live.w - tile0.w * 0.1).abs() < 1.0, "{live:?} {tile0:?}");
    }
}
