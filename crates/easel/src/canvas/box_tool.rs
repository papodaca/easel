use std::sync::Arc;

use easel_core::geometry::Point;
use easel_core::providers::bfl::flux3::compile_prompt;
use easel_core::scene::BoxKind;
use gpui::{
    CursorStyle, Focusable as _, Modifiers, MouseButton, TestAppContext, VisualTestContext, point,
    prelude::*, px,
};

use crate::actions::{self, BoxTool, DeleteBox, SelectTool};
use crate::app_state::{AppState, Tool};
use crate::canvas::Canvas;
use crate::notifications::Notifications;
use crate::settings::SharedKeys;
use crate::side_panel::SidePanel;
use crate::theme;
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
            let side_panel = cx.new(|cx| SidePanel::new(app_state.clone(), keys, None, None, cx));
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

fn sized_image(width: u32, height: u32) -> Arc<crate::image_load::DecodedImage> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pixel.png"
    ))
    .unwrap();
    let mut image = crate::image_load::decode_image(&bytes, "pixel.png").unwrap();
    image.size = easel_core::geometry::Size {
        w: width,
        h: height,
    };
    Arc::new(image)
}

fn window_point(canvas: &Canvas, index: usize, dx: f32, dy: f32) -> gpui::Point<gpui::Pixels> {
    let view = canvas.tile_view(index).expect("tile");
    let bounds = canvas.content_bounds();
    point(
        bounds.origin.x + px(view.x + dx),
        bounds.origin.y + px(view.y + dy),
    )
}

fn click_tile(cx: &mut VisualTestContext, canvas: &gpui::Entity<Canvas>, index: usize) {
    let at = canvas.read_with(cx, |canvas, _| {
        let view = canvas.tile_view(index).expect("tile");
        window_point(canvas, index, view.w / 2.0, view.h / 2.0)
    });
    cx.simulate_mouse_down(at, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Default::default());
}

#[gpui::test]
fn empty_hint_mentions_a_prompt_only_when_zero_images_are_allowed(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    let hint = root.read_with(&cx, |surface, cx| {
        surface.canvas.read(cx).hint_text(cx).unwrap()
    });
    assert!(hint.contains("A prompt can run with no image"), "{hint}");

    root.update(&mut cx, |surface, cx| {
        surface.app_state.update(cx, |app, cx| {
            app.set_model("krea-3", cx);
            app.values
                .insert("regional_edit".into(), serde_json::json!(true));
            cx.notify();
        });
    });
    let hint = root.read_with(&cx, |surface, cx| {
        surface.canvas.read(cx).hint_text(cx).unwrap()
    });
    assert!(hint.contains("Open, drop, or paste an image"), "{hint}");
    assert!(!hint.contains("A prompt can run with no image"), "{hint}");
}

#[gpui::test]
fn an_empty_sheet_with_an_aspect_draws_on_the_virtual_frame(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface
            .app_state
            .update(cx, |app, cx| app.set_aspect_ratio("1:1", cx));
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    assert_eq!(canvas.read_with(&cx, |canvas, _| canvas.tile_count()), 0);
    use_box_tool(&mut cx, &canvas);
    drag(&mut cx, &canvas, (40.0, 30.0), (160.0, 110.0));
    let rect = canvas.read_with(&cx, |canvas, cx| {
        canvas.app_state().read(cx).scene.boxes[0]
            .src
            .or(canvas.app_state().read(cx).scene.boxes[0].tgt)
            .unwrap()
    });
    assert!(rect.x >= 0 && rect.y >= 0);
    assert!(rect.x + rect.w <= 1000 && rect.y + rect.h <= 1000);
}

#[gpui::test]
fn a_click_selects_the_tile_under_the_pointer(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface.app_state.update(cx, |app, cx| {
            app.set_image(sized_image(100, 100), cx);
            app.add_extra(sized_image(400, 200), cx).unwrap();
        });
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    use_box_tool(&mut cx, &canvas);
    let start = canvas.read_with(&cx, |canvas, _| window_point(canvas, 0, 8.0, 8.0));
    let end = canvas.read_with(&cx, |canvas, _| window_point(canvas, 0, 40.0, 40.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    assert_eq!(
        canvas.read_with(&cx, |canvas, cx| {
            canvas.app_state().read(cx).scene.boxes.len()
        }),
        1
    );
    cx.update(|window, cx| {
        canvas.update(cx, |canvas, cx| canvas.focus_handle(cx).focus(window, cx));
        window.dispatch_action(Box::new(SelectTool), cx);
    });
    click_tile(&mut cx, &canvas, 1);
    let (tile, border, selected_box) = canvas.read_with(&cx, |canvas, cx| {
        (
            canvas.selected_tile(),
            canvas.tile_border(1),
            canvas.app_state().read(cx).selected_box.clone(),
        )
    });
    assert_eq!(tile, Some(1));
    assert_eq!(border, Some(theme::ACCENT));
    assert!(selected_box.is_none());
}

#[gpui::test]
fn a_drag_on_the_second_tile_stays_in_that_tiles_pixels(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface.app_state.update(cx, |app, cx| {
            app.set_image(sized_image(100, 100), cx);
            app.add_extra(sized_image(400, 200), cx).unwrap();
        });
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    use_box_tool(&mut cx, &canvas);
    click_tile(&mut cx, &canvas, 1);
    assert_eq!(
        canvas.read_with(&cx, |canvas, _| canvas.selected_tile()),
        Some(1)
    );
    assert!(canvas.read_with(&cx, |canvas, cx| {
        canvas.app_state().read(cx).scene.boxes.is_empty()
    }));

    let start = canvas.read_with(&cx, |canvas, _| window_point(canvas, 1, 12.0, 12.0));
    let end = canvas.read_with(&cx, |canvas, _| window_point(canvas, 1, 80.0, 50.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    let element = canvas.read_with(&cx, |canvas, cx| {
        canvas.app_state().read(cx).scene.boxes[0].clone()
    });
    assert_eq!(element.from_image, Some(1));
    let rect = element.tgt.unwrap();
    assert!(rect.x >= 0 && rect.y >= 0);
    assert!(rect.x + rect.w <= 400 && rect.y + rect.h <= 200);

    let (from_second, onto_first) = canvas.read_with(&cx, |canvas, _| {
        let second = canvas.tile_view(1).unwrap();
        (
            window_point(canvas, 1, second.w - 16.0, second.h - 16.0),
            window_point(canvas, 0, 10.0, 10.0),
        )
    });
    cx.simulate_mouse_down(from_second, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(onto_first, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(onto_first, MouseButton::Left, Default::default());
    let crossed = canvas.read_with(&cx, |canvas, cx| {
        canvas.app_state().read(cx).scene.boxes[1].clone()
    });
    assert_eq!(crossed.from_image, Some(1));
    let rect = crossed.tgt.unwrap();
    assert!(rect.x >= 0 && rect.y >= 0);
    assert!(rect.x + rect.w <= 400 && rect.y + rect.h <= 200);
}

#[gpui::test]
fn fit_keeps_both_tiles_inside_the_view(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface.app_state.update(cx, |app, cx| {
            app.set_image(sized_image(100, 100), cx);
            app.add_extra(sized_image(400, 200), cx).unwrap();
        });
    });
    let canvas = root.read_with(&cx, |surface, _| surface.canvas.clone());
    cx.update(|window, cx| {
        canvas.update(cx, |canvas, cx| canvas.focus_handle(cx).focus(window, cx));
        window.dispatch_action(Box::new(crate::actions::Fit), cx);
    });
    let (view, first, second) = canvas.read_with(&cx, |canvas, _| {
        (
            canvas.viewport().view_size,
            canvas.tile_view(0).unwrap(),
            canvas.tile_view(1).unwrap(),
        )
    });
    assert!(view.w > 0 && view.h > 0);
    for tile in [first, second] {
        assert!(tile.x >= -0.5, "{tile:?}");
        assert!(tile.y >= -0.5, "{tile:?}");
        assert!(tile.x + tile.w <= view.w as f32 + 1.0, "{tile:?} {view:?}");
        assert!(tile.y + tile.h <= view.h as f32 + 1.0, "{tile:?} {view:?}");
    }
    assert!(second.x > first.x);
}

#[gpui::test]
fn the_second_tile_label_matches_its_slot_mark(cx: &mut TestAppContext) {
    let (window, mut cx) = open(cx);
    let root = surface(&window, &mut cx);
    root.update(&mut cx, |surface, cx| {
        surface.app_state.update(cx, |app, cx| {
            app.set_image(sized_image(100, 100), cx);
            app.add_extra(sized_image(400, 200), cx).unwrap();
        });
    });
    let (label, mark) = root.read_with(&cx, |surface, cx| {
        let label = surface.canvas.read(cx).tile_label(1, cx).unwrap();
        let mark = surface.app_state.read(cx).reference_marks()[1].0.clone();
        (label, mark)
    });
    assert_eq!(label, mark);

    root.update(&mut cx, |surface, cx| {
        surface.app_state.update(cx, |app, cx| {
            let max = app.spec().capabilities.image_slots.max();
            while usize::from(app.scene.source_image.is_some()) + app.scene.references.len() <= max
            {
                let image = sized_image(20, 20);
                app.scene.add_reference(easel_core::scene::ImageRef::new(
                    "reference",
                    image.size.w,
                    image.size.h,
                ));
                app.extras.push(image);
            }
            cx.notify();
        });
    });
    let (label, sent) = root.read_with(&cx, |surface, cx| {
        let app = surface.app_state.read(cx);
        let index = app.reference_marks().len() - 1;
        let label = surface.canvas.read(cx).tile_label(index, cx).unwrap();
        let sent = app.reference_marks()[index].1;
        (label, sent)
    });
    assert!(!sent);
    assert!(label.contains("not sent"), "{label}");
}
