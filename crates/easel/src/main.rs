mod actions;
mod app_state;
mod assets;
mod bottom_strip;
mod canvas;
mod chrome;
mod image_load;
mod jobs;
mod net;
mod notifications;
mod root;
mod session;
mod settings;
mod side_panel;
mod status_bar;
mod theme;
mod toolbar;
mod ui;
mod widgets;

use gpui::{
    App, Bounds, Focusable as _, TitlebarOptions, WindowBounds, WindowDecorations, WindowOptions,
    WindowingModes, prelude::*, px, size,
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
    let _ = jobs::data_dir();
    let _ = log::set_logger(&STDERR_LOG);
    log::set_max_level(log::LevelFilter::Debug);
    linux(WindowingModes::WAYLAND | WindowingModes::X11)
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_tokio::init(cx);
            assets::load_fonts(cx);
            assets::install_shell_icon();
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
                    window_decorations: Some(WindowDecorations::Client),
                    window_min_size: Some(size(px(720.0), px(480.0))),
                    app_id: Some(assets::app_id().to_owned()),
                    icon: assets::window_icon(),
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
