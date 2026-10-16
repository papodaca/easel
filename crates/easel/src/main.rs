use gpui::{
    App, Bounds, Context, SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions,
    WindowingModes, div, prelude::*, px, size,
};
use gpui_platform::linux;

struct Easel;

impl Render for Easel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("easel")
    }
}

fn main() {
    linux(WindowingModes::WAYLAND | WindowingModes::X11).run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("easel")),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Easel),
        )
        .unwrap();
        cx.activate(true);
    });
}
