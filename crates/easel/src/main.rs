mod actions;
mod app_state;
mod bottom_strip;
mod canvas;
mod image_load;
mod net;
mod notifications;
mod root;
mod side_panel;
mod theme;
mod toolbar;
mod widgets;

use gpui::{
    App, Bounds, Focusable as _, TitlebarOptions, WindowBounds, WindowOptions, WindowingModes,
    prelude::*, px, size,
};
use gpui_platform::linux;

use crate::root::Root;

struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target().starts_with("easel")
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static STDERR_LOG: StderrLog = StderrLog;

fn main() {
    let _ = log::set_logger(&STDERR_LOG);
    log::set_max_level(log::LevelFilter::Debug);
    linux(WindowingModes::WAYLAND | WindowingModes::X11).run(|cx: &mut App| {
        gpui_tokio::init(cx);
        actions::bind_keys(cx);
        widgets::bind_keys(cx);
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
