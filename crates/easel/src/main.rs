mod actions;
mod app_state;
mod bottom_strip;
mod canvas;
mod image_load;
mod notifications;
mod root;
mod side_panel;
mod theme;
mod toolbar;

use gpui::{
    App, Bounds, Focusable as _, TitlebarOptions, WindowBounds, WindowOptions, WindowingModes,
    prelude::*, px, size,
};
use gpui_platform::linux;

use crate::root::Root;

fn main() {
    linux(WindowingModes::WAYLAND | WindowingModes::X11).run(|cx: &mut App| {
        actions::bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("easel".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let root = cx.new(Root::new);
                root.focus_handle(cx).focus(window, cx);
                root
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
