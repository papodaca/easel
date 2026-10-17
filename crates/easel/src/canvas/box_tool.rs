use std::sync::Arc;

use easel_core::geometry::Point;
use easel_core::providers::bfl::flux3::compile_prompt;
use easel_core::scene::BoxKind;
use gpui::{
    CursorStyle, Focusable as _, Modifiers, MouseButton, TestAppContext, VisualTestContext, point,
    prelude::*, px,
};

use crate::actions::{self, BoxTool, DeleteBox};
use crate::app_state::{AppState, Tool};
use crate::canvas::Canvas;
use crate::notifications::Notifications;
use crate::settings::SharedKeys;
use crate::side_panel::SidePanel;
use crate::toolbar::Toolbar;

struct Surface {
    app_state: gpui::Entity<AppState>,
    toolbar: gpui::Entity<Toolbar>,
    canvas: gpui::Entity<Canvas>,
    side_panel: gpui::Entity<SidePanel>,
}

impl gpui::Render for Surface {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.toolbar.clone())
            .child(
                gpui::div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.canvas.clone())
                    .child(self.side_panel.clone()),
            )
    }
}

impl gpui::Focusable for Surface {
    fn focus_handle(&self, cx: &gpui::App) -> gpui::FocusHandle {
        self.canvas.focus_handle(cx)
    }
}

fn open(cx: &mut TestAppContext) -> (gpui::WindowHandle<Surface>, VisualTestContext) {
    let window = cx.update(|cx| {
        actions::bind_keys(cx);
        crate::widgets::bind_keys(cx);
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            let app_state = cx.new(|_| AppState::new());
            let notes = cx.new(|_| Notifications::new());
            let toolbar = cx.new(|_| Toolbar::new(app_state.clone()));
            let canvas = cx.new(|cx| Canvas::new(app_state.clone(), notes.clone(), cx));
            let keys = Arc::new(SharedKeys::default());
            let side_panel = cx.new(|cx| SidePanel::new(app_state.clone(), keys, None, cx));
            let surface = cx.new(|_| Surface {
                app_state,
                toolbar,
                canvas,
                side_panel,
            });
            surface.focus_handle(cx).focus(window, cx);
            surface
        })
        .unwrap()
    });
    let visual = VisualTestContext::from_window(window.into(), cx);
    (window, visual)
}

fn surface(
    window: &gpui::WindowHandle<Surface>,
    cx: &mut VisualTestContext,
) -> gpui::Entity<Surface> {
    window.root(cx).unwrap()
}

fn drag(
    cx: &mut VisualTestContext,
    canvas: &gpui::Entity<Canvas>,
    from: (f32, f32),
    to: (f32, f32),
) {
    let (start, end) = canvas.read_with(cx, |canvas, _| {
        let bounds = canvas.content_bounds();
        let image = canvas.viewport().image_rect_in_view();
        (
            point(
                bounds.origin.x + px(image.x + from.0),
                bounds.origin.y + px(image.y + from.1),
            ),
            point(
                bounds.origin.x + px(image.x + to.0),
                bounds.origin.y + px(image.y + to.1),
            ),
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
}

fn source_image() -> Arc<crate::image_load::DecodedImage> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pixel.png"
    ))
    .unwrap();
    let mut image = crate::image_load::decode_image(&bytes, "pixel.png").unwrap();
    image.size = easel_core::geometry::Size { w: 1000, h: 1000 };
    Arc::new(image)
}

fn image_of(canvas: &Canvas, local: (f32, f32)) -> Point {
    let image = canvas.viewport().image_rect_in_view();
    canvas.viewport().view_to_image(Point {
        x: image.x + local.0,
        y: image.y + local.1,
    })
}

fn use_box_tool(cx: &mut VisualTestContext, canvas: &gpui::Entity<Canvas>) {
    let canvas = canvas.clone();
    cx.update(|window, cx| {
        canvas.update(cx, |canvas, cx| {
            canvas.focus_handle(cx).focus(window, cx);
        });
        window.dispatch_action(Box::new(BoxTool), cx);
    });
}

#[gpui::test]
fn drag_creates_one_box_in_image_space(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface.app_state.update(cx, |app, cx| {
            app.set_aspect_ratio("1:1", cx);
        });
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    use_box_tool(&mut cx, &canvas);
    drag(&mut cx, &canvas, (40.0, 30.0), (160.0, 110.0));
    let (stored, expected) = canvas.read_with(&cx, |canvas, cx| {
        let scene = &canvas.app_state().read(cx).scene;
        let element = scene.boxes.first().expect("one box");
        let start = image_of(canvas, (40.0, 30.0));
        let end = image_of(canvas, (160.0, 110.0));
        let frame = scene.frame().unwrap();
        let expected = easel_core::geometry::Rect {
            x: start.x.round() as i32,
            y: start.y.round() as i32,
            w: (end.x - start.x).round() as i32,
            h: (end.y - start.y).round() as i32,
        }
        .normalize()
        .clamp_to(frame);
        (element.tgt.or(element.src).unwrap(), expected)
    });
    assert_eq!(stored, expected);
    assert!(stored.w > 2 && stored.h > 2, "{stored:?}");
    assert_eq!(
        canvas.read_with(&cx, |canvas, cx| canvas
            .app_state()
            .read(cx)
            .scene
            .boxes
            .len()),
        1
    );
}

#[gpui::test]
fn the_box_is_visible_before_the_mouse_comes_up(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_aspect_ratio("1:1", cx));
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    use_box_tool(&mut cx, &canvas);
    let (start, mid) = canvas.read_with(&cx, |canvas, _| {
        let bounds = canvas.content_bounds();
        let image = canvas.viewport().image_rect_in_view();
        (
            point(
                bounds.origin.x + px(image.x + 40.0),
                bounds.origin.y + px(image.y + 30.0),
            ),
            point(
                bounds.origin.x + px(image.x + 120.0),
                bounds.origin.y + px(image.y + 90.0),
            ),
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(mid, MouseButton::Left, Modifiers::default());
    let (count, draft) = canvas.read_with(&cx, |canvas, cx| {
        (
            canvas.app_state().read(cx).scene.boxes.len(),
            canvas.draft_rect().map(|rect| rect.w > 2 && rect.h > 2),
        )
    });
    assert_eq!(count, 0);
    assert_eq!(draft, Some(true));
}

#[gpui::test]
fn source_image_makes_a_new_box_and_a_frame_makes_layout(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_aspect_ratio("1:1", cx));
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    use_box_tool(&mut cx, &canvas);
    drag(&mut cx, &canvas, (40.0, 40.0), (160.0, 140.0));
    let kind = canvas.read_with(&cx, |canvas, cx| {
        canvas.app_state().read(cx).scene.boxes[0].kind
    });
    assert_eq!(kind, BoxKind::Layout);

    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_image(source_image(), cx));
    });
    drag(&mut cx, &canvas, (70.0, 70.0), (150.0, 130.0));
    let kinds = canvas.read_with(&cx, |canvas, cx| {
        canvas
            .app_state()
            .read(cx)
            .scene
            .boxes
            .iter()
            .map(|element| element.kind)
            .collect::<Vec<_>>()
    });
    assert!(kinds.contains(&BoxKind::New), "{kinds:?}");
}

#[gpui::test]
fn delete_clears_the_inspector(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_aspect_ratio("1:1", cx));
    });
    let (canvas, inspector) = root.read_with(&cx, |surface, cx| {
        (
            surface.canvas.clone(),
            surface.side_panel.read(cx).inspector().clone(),
        )
    });
    use_box_tool(&mut cx, &canvas);
    drag(&mut cx, &canvas, (40.0, 40.0), (140.0, 120.0));
    cx.run_until_parked();
    assert!(inspector.read_with(&cx, |inspector, _| inspector.shows_box()));
    let canvas_for_focus = canvas.clone();
    cx.update(|window, cx| {
        canvas_for_focus.update(cx, |canvas, cx| {
            canvas.focus_handle(cx).focus(window, cx);
        });
        window.dispatch_action(Box::new(DeleteBox), cx);
    });
    cx.run_until_parked();
    assert!(canvas.read_with(&cx, |canvas, cx| {
        canvas.app_state().read(cx).scene.boxes.is_empty()
    }));
    assert!(!inspector.read_with(&cx, |inspector, _| inspector.shows_box()));
}

#[gpui::test]
fn move_then_draw_sets_both_rects(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_image(source_image(), cx));
    });
    let (canvas, inspector) = root.read_with(&cx, |surface, cx| {
        (
            surface.canvas.clone(),
            surface.side_panel.read(cx).inspector().clone(),
        )
    });
    use_box_tool(&mut cx, &canvas);
    drag(&mut cx, &canvas, (30.0, 30.0), (120.0, 100.0));
    inspector.update(&mut cx, |inspector, cx| {
        inspector.set_kind(BoxKind::Move, cx)
    });
    drag(&mut cx, &canvas, (160.0, 40.0), (260.0, 130.0));
    let (src, tgt, text) = canvas.read_with(&cx, |canvas, cx| {
        let scene = &canvas.app_state().read(cx).scene;
        let element = &scene.boxes[0];
        let frame = scene.frame().unwrap();
        let compiled = compile_prompt(scene, frame);
        (element.src, element.tgt, compiled.text)
    });
    assert!(src.is_some() && tgt.is_some());
    assert_ne!(src, tgt);
    assert!(text.contains("src_bbox"), "{text}");
    assert!(text.contains("tgt_bbox"), "{text}");
}

#[gpui::test]
fn move_then_escape_restores_the_new_box(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_image(source_image(), cx));
    });
    let (canvas, inspector) = root.read_with(&cx, |surface, cx| {
        (
            surface.canvas.clone(),
            surface.side_panel.read(cx).inspector().clone(),
        )
    });
    use_box_tool(&mut cx, &canvas);
    drag(&mut cx, &canvas, (30.0, 30.0), (120.0, 100.0));
    let before = canvas.read_with(&cx, |canvas, cx| {
        let element = &canvas.app_state().read(cx).scene.boxes[0];
        (element.kind, element.src, element.tgt)
    });
    inspector.update(&mut cx, |inspector, cx| {
        inspector.set_kind(BoxKind::Move, cx)
    });
    let canvas_for_focus = canvas.clone();
    cx.update(|window, cx| {
        canvas_for_focus.update(cx, |canvas, cx| {
            canvas.focus_handle(cx).focus(window, cx);
        });
    });
    cx.simulate_keystrokes("escape");
    let after = canvas.read_with(&cx, |canvas, cx| {
        let app = canvas.app_state().read(cx);
        let element = &app.scene.boxes[0];
        (
            element.kind,
            element.src,
            element.tgt,
            app.draw_target.clone(),
        )
    });
    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2);
    assert!(after.3.is_none());
}

#[gpui::test]
fn renaming_from_the_id_field_uses_the_next_suffix(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_aspect_ratio("1:1", cx));
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    use_box_tool(&mut cx, &canvas);
    drag(&mut cx, &canvas, (40.0, 40.0), (150.0, 130.0));
    cx.run_until_parked();
    cx.simulate_input("cat");
    cx.simulate_keystrokes("enter");
    let (id, field) = root.read_with(&cx, |surface, cx| {
        let id = surface.app_state.read(cx).scene.boxes[0]
            .id
            .as_str()
            .to_string();
        let field = surface.side_panel.read(cx).inspector().read(cx).id_text(cx);
        (id, field)
    });
    assert_eq!(id, "cat_1");
    assert_eq!(field, "cat_1");
}

#[gpui::test]
fn auto_without_a_frame_disables_the_box_tool(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    let (enabled, hint) = root.read_with(&cx, |surface, cx| {
        (
            surface.toolbar.read(cx).box_tool_enabled(cx),
            surface.canvas.read(cx).shows_hint(cx),
        )
    });
    assert!(!enabled);
    assert!(hint);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_aspect_ratio("1:1", cx));
    });
    let (enabled, hint, tool) = root.read_with(&cx, |surface, cx| {
        (
            surface.toolbar.read(cx).box_tool_enabled(cx),
            surface.canvas.read(cx).shows_hint(cx),
            surface.app_state.read(cx).tool,
        )
    });
    assert!(enabled);
    assert!(!hint);
    assert_eq!(tool, Tool::Select);
}

#[gpui::test]
fn a_selected_handle_shows_the_resize_cursor(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_aspect_ratio("1:1", cx));
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    use_box_tool(&mut cx, &canvas);
    drag(&mut cx, &canvas, (40.0, 30.0), (160.0, 110.0));
    let (corner, north, east, inside) = canvas.read_with(&cx, |canvas, cx| {
        let rect = canvas.app_state().read(cx).scene.boxes[0]
            .src
            .or(canvas.app_state().read(cx).scene.boxes[0].tgt)
            .unwrap();
        let viewport = canvas.viewport();
        let view = super::view_of(&viewport, rect);
        let bounds = canvas.content_bounds();
        let at = |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
        (
            at(view.x, view.y),
            at(view.x + view.w / 2.0, view.y),
            at(view.x + view.w, view.y + view.h / 2.0),
            at(view.x + view.w / 2.0, view.y + view.h / 2.0),
        )
    });
    cx.simulate_mouse_move(corner, None, Modifiers::default());
    assert_eq!(
        canvas.read_with(&cx, |canvas, _| canvas.pointer()),
        CursorStyle::ResizeUpLeftDownRight
    );
    cx.simulate_mouse_move(north, None, Modifiers::default());
    assert_eq!(
        canvas.read_with(&cx, |canvas, _| canvas.pointer()),
        CursorStyle::ResizeUpDown
    );
    cx.simulate_mouse_move(east, None, Modifiers::default());
    assert_eq!(
        canvas.read_with(&cx, |canvas, _| canvas.pointer()),
        CursorStyle::ResizeLeftRight
    );
    cx.simulate_mouse_move(inside, None, Modifiers::default());
    assert_eq!(
        canvas.read_with(&cx, |canvas, _| canvas.pointer()),
        CursorStyle::Crosshair
    );
}
