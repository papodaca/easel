//! Window chrome: client-side decorations (rounded corners, 1 px border, drop
//! shadow, resize edges) and the title bar with Linux window controls.
//!
//! The decoration technique follows Zed's `crates/gpui/examples/window_shadow.rs`
//! at v1.24.2 (Apache-2.0). On a compositor that insists on server-side
//! decorations, `window.window_decorations()` reports `Server` and everything
//! here degrades to a plain title row.

use gpui::{
    App, Bounds, Context, CursorStyle, Decorations, FontWeight, HitboxBehavior, Hsla, IntoElement,
    MouseButton, ParentElement, Pixels, Point, Render, ResizeEdge, Size, Styled, Tiling, Window,
    WindowButton, WindowButtonLayout, canvas, div, point, prelude::*, px, rgb, svg,
    transparent_black,
};

use crate::actions::Quit;
use crate::theme;

/// Wrap the window content in the decoration frame. `on_edge_change` fires when
/// the hovered resize edge changes so the owner can `cx.notify()` and the cursor
/// updates without redrawing on every mouse move.
pub fn client_side_decorations(
    content: impl IntoElement,
    hovered_edge: Option<ResizeEdge>,
    on_edge_change: impl Fn(Option<ResizeEdge>, &mut Window, &mut App) + 'static,
    window: &mut Window,
) -> gpui::Stateful<gpui::Div> {
    let decorations = window.window_decorations();
    let resizable = window.is_resizable();
    let tiling = match decorations {
        Decorations::Server => Tiling::default(),
        Decorations::Client { tiling } => tiling,
    };
    match decorations {
        Decorations::Client { .. } => window.set_client_inset(theme::WINDOW_SHADOW),
        Decorations::Server => window.set_client_inset(px(0.)),
    }
    let client = matches!(decorations, Decorations::Client { .. });

    div()
        .id("window-backdrop")
        .size_full()
        .bg(transparent_black())
        .when(client, |backdrop| {
            backdrop
                .when(!tiling.top, |div| div.pt(theme::WINDOW_SHADOW))
                .when(!tiling.bottom, |div| div.pb(theme::WINDOW_SHADOW))
                .when(!tiling.left, |div| div.pl(theme::WINDOW_SHADOW))
                .when(!tiling.right, |div| div.pr(theme::WINDOW_SHADOW))
                .when(resizable, |div| {
                    div.on_mouse_move(move |event, window, cx| {
                        let size = window.window_bounds().get_bounds().size;
                        let edge = resize_edge(event.position, theme::WINDOW_SHADOW, size, tiling);
                        if edge != hovered_edge {
                            on_edge_change(edge, window, cx);
                        }
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        move |event, window, _| {
                            let size = window.window_bounds().get_bounds().size;
                            if let Some(edge) =
                                resize_edge(event.position, theme::WINDOW_SHADOW, size, tiling)
                            {
                                window.start_window_resize(edge);
                            }
                        },
                    )
                })
        })
        .child(
            div()
                .size_full()
                .cursor(CursorStyle::Arrow)
                .overflow_hidden()
                .when(client, |frame| {
                    frame
                        .border_color(rgb(theme::BORDER))
                        .map(|frame| round_client_corners(frame, tiling))
                        .when(!tiling.top, |div| div.border_t(theme::WINDOW_BORDER))
                        .when(!tiling.bottom, |div| div.border_b(theme::WINDOW_BORDER))
                        .when(!tiling.left, |div| div.border_l(theme::WINDOW_BORDER))
                        .when(!tiling.right, |div| div.border_r(theme::WINDOW_BORDER))
                        .when(!tiling.is_tiled(), |div| {
                            div.shadow(vec![
                                gpui::BoxShadow::new(
                                    px(0.),
                                    px(0.),
                                    Hsla {
                                        h: 0.,
                                        s: 0.,
                                        l: 0.,
                                        a: 0.4,
                                    },
                                )
                                .blur_radius(theme::WINDOW_SHADOW / 2.),
                            ])
                        })
                })
                .on_mouse_move(|_, _, cx| cx.stop_propagation())
                .child(content),
        )
        .when(client && resizable, |backdrop| {
            backdrop.child(
                canvas(
                    |_, window, _| {
                        window.insert_hitbox(
                            Bounds::new(
                                point(px(0.), px(0.)),
                                window.window_bounds().get_bounds().size,
                            ),
                            HitboxBehavior::Normal,
                        )
                    },
                    move |_, hitbox, window, _| {
                        let Some(edge) = hovered_edge else {
                            return;
                        };
                        let cursor = match edge {
                            ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                            ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
                            ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                CursorStyle::ResizeUpLeftDownRight
                            }
                            ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                CursorStyle::ResizeUpRightDownLeft
                            }
                        };
                        window.set_cursor_style(cursor, &hitbox);
                    },
                )
                .absolute()
                .size_full(),
            )
        })
}

/// The window's edge tiling, or an untiled default under server decorations.
pub fn tiling(window: &Window) -> Tiling {
    match window.window_decorations() {
        Decorations::Server => Tiling::default(),
        Decorations::Client { tiling } => tiling,
    }
}

pub fn is_client_decorated(window: &Window) -> bool {
    matches!(window.window_decorations(), Decorations::Client { .. })
}

pub fn round_client_corners<E: Styled>(element: E, tiling: Tiling) -> E {
    let mut element = element;
    if !(tiling.top || tiling.left) {
        element = element.rounded_tl(theme::WINDOW_ROUNDING);
    }
    if !(tiling.top || tiling.right) {
        element = element.rounded_tr(theme::WINDOW_ROUNDING);
    }
    if !(tiling.bottom || tiling.left) {
        element = element.rounded_bl(theme::WINDOW_ROUNDING);
    }
    if !(tiling.bottom || tiling.right) {
        element = element.rounded_br(theme::WINDOW_ROUNDING);
    }
    element
}

fn resize_edge(
    position: Point<Pixels>,
    shadow: Pixels,
    window_size: Size<Pixels>,
    tiling: Tiling,
) -> Option<ResizeEdge> {
    let inner = Bounds::new(Point::default(), window_size).inset(shadow * 1.5);
    if inner.contains(&position) {
        return None;
    }
    let corner = shadow * 1.5;
    let near_top = !tiling.top && position.y < corner;
    let near_bottom = !tiling.bottom && position.y > window_size.height - corner;
    let near_left = !tiling.left && position.x < corner;
    let near_right = !tiling.right && position.x > window_size.width - corner;
    match (near_top, near_bottom, near_left, near_right) {
        (true, _, true, _) => Some(ResizeEdge::TopLeft),
        (true, _, _, true) => Some(ResizeEdge::TopRight),
        (_, true, true, _) => Some(ResizeEdge::BottomLeft),
        (_, true, _, true) => Some(ResizeEdge::BottomRight),
        _ => {
            if !tiling.top && position.y < shadow {
                Some(ResizeEdge::Top)
            } else if !tiling.bottom && position.y > window_size.height - shadow {
                Some(ResizeEdge::Bottom)
            } else if !tiling.left && position.x < shadow {
                Some(ResizeEdge::Left)
            } else if !tiling.right && position.x > window_size.width - shadow {
                Some(ResizeEdge::Right)
            } else {
                None
            }
        }
    }
}

/// The title bar: app name on the left and the desktop's window controls when
/// we draw our own decorations. Drag to move, double-click to maximize,
/// right-click for the window menu.
pub struct TitleBar {
    should_move: bool,
}

impl TitleBar {
    pub fn new() -> Self {
        Self { should_move: false }
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let client = is_client_decorated(window);
        let tiling = tiling(window);
        let supported = window.window_controls();
        let layout = client.then(|| cx.button_layout().unwrap_or_else(linux_default_layout));

        let left_controls = layout
            .filter(|layout| layout.left[0].is_some())
            .map(|layout| window_controls("left-window-controls", layout.left, window));
        let right_controls = layout
            .filter(|layout| layout.right[0].is_some())
            .map(|layout| window_controls("right-window-controls", layout.right, window));

        div()
            .id("title-bar")
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .w_full()
            .h(px(theme::TITLE_BAR_HEIGHT))
            .bg(rgb(theme::TITLE_BAR))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .text_sm()
            .text_color(rgb(theme::TEXT))
            .when(client, |bar| {
                bar.map(|bar| {
                    round_client_corners(
                        bar,
                        Tiling {
                            bottom: true,
                            ..tiling
                        },
                    )
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.should_move = true),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.should_move = false),
                )
                .on_mouse_down_out(cx.listener(|this, _, _, _| this.should_move = false))
                .on_mouse_move(cx.listener(|this, _, window, _| {
                    if this.should_move {
                        this.should_move = false;
                        window.start_window_move();
                    }
                }))
                .when(supported.maximize && window.is_resizable(), |bar| {
                    bar.on_click(|event, window, _| {
                        if event.click_count() == 2 {
                            window.zoom_window();
                        }
                    })
                })
                .when(supported.window_menu, |bar| {
                    bar.on_mouse_down(MouseButton::Right, |event, window, _| {
                        window.show_window_menu(event.position);
                    })
                })
            })
            .children(left_controls)
            .when(
                layout.is_none_or(|layout| layout.left[0].is_none()),
                |bar| bar.pl(px(12.)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .items_center()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .pr(px(12.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::TEXT))
                            .child("easel"),
                    ),
            )
            .children(right_controls)
    }
}

fn linux_default_layout() -> WindowButtonLayout {
    WindowButtonLayout {
        left: [None, None, None],
        right: [
            Some(WindowButton::Minimize),
            Some(WindowButton::Maximize),
            Some(WindowButton::Close),
        ],
    }
}

fn window_controls(
    id: &'static str,
    buttons: [Option<WindowButton>; 3],
    window: &Window,
) -> gpui::Stateful<gpui::Div> {
    let supported = window.window_controls();
    let maximized = window.is_maximized();
    let controls = buttons
        .into_iter()
        .flatten()
        .filter(|button| match button {
            WindowButton::Minimize => supported.minimize,
            WindowButton::Maximize => supported.maximize,
            WindowButton::Close => true,
        })
        .map(|button| window_control(button, maximized, window));
    div()
        .id(id)
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(12.))
        .px(px(12.))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(controls)
}

fn window_control(
    button: WindowButton,
    maximized: bool,
    window: &Window,
) -> gpui::Stateful<gpui::Div> {
    let (id, icon, enabled) = match button {
        WindowButton::Minimize => ("minimize", "icons/minimize.svg", window.is_minimizable()),
        WindowButton::Maximize if maximized => ("restore", "icons/restore.svg", true),
        WindowButton::Maximize => ("maximize", "icons/maximize.svg", window.is_resizable()),
        WindowButton::Close => ("close", "icons/close.svg", true),
    };
    let icon_color = if enabled {
        theme::TEXT
    } else {
        theme::TEXT_DISABLED
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(20.))
        .rounded_full()
        .when(enabled, |control| {
            control
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme::ELEMENT_HOVER)))
                .active(|style| style.bg(rgb(theme::ELEMENT_ACTIVE)))
        })
        .child(
            svg()
                .size(px(16.))
                .flex_none()
                .path(icon)
                .text_color(rgb(icon_color)),
        )
        .on_mouse_move(|_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            if !enabled {
                return;
            }
            match button {
                WindowButton::Minimize => window.minimize_window(),
                WindowButton::Maximize => window.zoom_window(),
                WindowButton::Close => window.dispatch_action(Box::new(Quit), cx),
            }
        })
}
