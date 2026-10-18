use gpui::{
    App, Context, ExternalPaths, FocusHandle, Focusable, IntoElement, MouseButton, ParentElement,
    PathPromptOptions, Render, ResizeEdge, Styled, Window, div, img, prelude::*, px, rgb,
};

use crate::actions::{ActualSize, CloseSettings, Fit, Open, Paste, Quit, SaveAs, Settings};
use crate::app_state::AppState;
use crate::assets;
use crate::bottom_strip::{BottomStrip, HistoryDialog};
use crate::canvas::Canvas;
use crate::chrome::{self, TitleBar};
use crate::image_load::{self, prompt_failure_text};
use crate::jobs::JobRunner;
use crate::net::{HttpTransport, ReqwestTransport};
use crate::notifications::Notifications;
use crate::settings::{Resolver, SettingsPanel, SharedKeys};
use crate::side_panel::SidePanel;
use crate::status_bar::StatusBar;
use crate::theme;
use crate::toolbar::Toolbar;
use crate::ui::{Button, pane_header};

pub struct Root {
    focus_handle: FocusHandle,
    pub app_state: gpui::Entity<AppState>,
    notifications: gpui::Entity<Notifications>,
    title_bar: gpui::Entity<TitleBar>,
    toolbar: gpui::Entity<Toolbar>,
    canvas: gpui::Entity<Canvas>,
    side_panel: gpui::Entity<SidePanel>,
    settings: gpui::Entity<SettingsPanel>,
    bottom_strip: gpui::Entity<BottomStrip>,
    status_bar: gpui::Entity<StatusBar>,
    #[allow(dead_code)]
    session: gpui::Entity<crate::session::Session>,
    hovered_edge: Option<ResizeEdge>,
}

impl Root {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::with_history(Self::open_store(), cx)
    }

    #[cfg(test)]
    pub fn test_open(
        history: std::sync::Arc<easel_core::history::HistoryStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_history(history, cx)
    }

    fn open_store() -> std::sync::Arc<easel_core::history::HistoryStore> {
        match easel_core::history::HistoryStore::open(crate::jobs::data_dir().join("history")) {
            Ok(store) => std::sync::Arc::new(store),
            Err(error) => {
                log::warn!("history store: {error}");
                std::sync::Arc::new(
                    easel_core::history::HistoryStore::open(
                        std::env::temp_dir().join("easel-history"),
                    )
                    .expect("temporary history store"),
                )
            }
        }
    }

    fn with_history(
        history: std::sync::Arc<easel_core::history::HistoryStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_state = cx.new(|_cx| AppState::new());
        let notifications = cx.new(|_cx| Notifications::new());
        let title_bar = cx.new(|_cx| TitleBar::new());
        let toolbar = cx.new(|_cx| Toolbar::new(app_state.clone()));
        let canvas = cx.new(|cx| Canvas::new(app_state.clone(), notifications.clone(), cx));
        let keys = std::sync::Arc::new(SharedKeys::default());
        let transport: std::sync::Arc<dyn HttpTransport> = std::sync::Arc::new(
            ReqwestTransport::new(Resolver(std::sync::Arc::clone(&keys))),
        );
        let settings_notes = notifications.clone();
        let settings_keys = std::sync::Arc::clone(&keys);
        let settings_transport = std::sync::Arc::clone(&transport);
        let settings = cx.new(move |cx| {
            SettingsPanel::open(settings_notes, settings_transport, settings_keys, cx)
        });
        let credit_transport = std::sync::Arc::clone(&transport);
        let runner_transport = transport;
        let runner_keys = std::sync::Arc::new(Resolver(std::sync::Arc::clone(&keys)));
        let credit_keys = std::sync::Arc::clone(&runner_keys);
        let strip_history = std::sync::Arc::clone(&history);
        let runner = cx.new(move |_| {
            JobRunner::new(
                easel_core::providers::resolve_model,
                runner_transport,
                runner_keys,
                history,
            )
        });
        runner.update(cx, |runner, _| {
            runner.watch_krea_sunset(app_state.clone(), notifications.clone());
        });
        let panel_runner = runner.clone();
        let credit_runner = runner.clone();
        let credit_settings = settings.clone();
        let status_bar = cx.new(|cx| {
            StatusBar::new(
                notifications.clone(),
                app_state.clone(),
                easel_core::model_spec::ProviderId::Bfl,
                credit_transport,
                credit_keys,
                Some(credit_runner),
                Some(credit_settings),
                cx,
            )
        });
        let strip_runner = runner;
        let data_directory = crate::jobs::data_dir().display().to_string();
        settings.update(cx, |settings, cx| {
            settings.set_data_directory(data_directory, cx);
        });
        let panel_state = app_state.clone();
        let strip_state = app_state.clone();
        let settings_view = settings.clone();
        let side_settings = settings.clone();
        let side_panel = cx.new(move |cx| {
            SidePanel::new(
                panel_state,
                keys,
                Some(panel_runner),
                Some(side_settings),
                cx,
            )
        });
        let bottom_strip =
            cx.new(move |cx| BottomStrip::new(strip_state, Some(strip_runner), strip_history, cx));
        let session_path = crate::jobs::data_dir().join("session.json");
        let image_dir = crate::jobs::data_dir().join("session");
        let watched = app_state.clone();
        let session = cx.new({
            let session_path = session_path.clone();
            move |cx| crate::session::Session::attach(watched, session_path, image_dir, cx)
        });
        crate::session::restore(app_state.clone(), session_path, cx);
        Self {
            focus_handle: cx.focus_handle(),
            app_state,
            notifications,
            title_bar,
            toolbar,
            canvas,
            side_panel,
            settings: settings_view,
            bottom_strip,
            status_bar,
            session,
            hovered_edge: None,
        }
    }

    fn settings_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-scrim")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x11111b, 0.72))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.app_state
                        .update(cx, |state, cx| state.set_settings_open(false, cx));
                    this.set_status("Ready", cx);
                }),
            )
            .child(
                div()
                    .id("settings-dialog")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex()
                    .flex_col()
                    .w(px(440.))
                    .max_h(px(560.))
                    .bg(rgb(theme::PANEL))
                    .border_1()
                    .border_color(rgb(theme::BORDER))
                    .rounded_md()
                    .overflow_hidden()
                    .text_color(rgb(theme::TEXT))
                    .child(
                        pane_header("Settings")
                            .child(Button::ghost("settings-close", "Close").action(CloseSettings)),
                    )
                    .child(
                        div()
                            .id("settings-body")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .p(px(16.))
                            .child(self.settings.clone()),
                    ),
            )
    }

    fn history_modal(&self, dialog: HistoryDialog) -> impl IntoElement {
        let strip = self.bottom_strip.clone();
        let image = dialog.image.clone();
        let mut details = vec![dialog.line.clone()];
        if let Some(cost) = dialog.cost {
            details.push(format!("{cost:.2}"));
        }
        if !dialog.detail.is_empty() {
            details.push(dialog.detail.clone());
        }
        if !dialog.model.is_empty() {
            details.push(dialog.model.clone());
        }
        if let Some((width, height)) = dialog.pixel_size {
            details.push(format!("{width} x {height}"));
        }
        let actions = dialog.actions.clone();
        let confirming = dialog.confirming;
        let prompt = dialog.prompt;
        div()
            .id("history-scrim")
            .debug_selector(|| "history-scrim".into())
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(0x11111b, 0.72))
            .on_mouse_down(MouseButton::Left, {
                let strip = strip.clone();
                move |_, _, cx| {
                    strip.update(cx, |strip, cx| strip.dismiss_modal(cx));
                }
            })
            .child(
                div()
                    .id("history-dialog")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex()
                    .flex_col()
                    .w(px(480.))
                    .max_h(px(640.))
                    .bg(rgb(theme::PANEL))
                    .border_1()
                    .border_color(rgb(theme::BORDER))
                    .rounded_md()
                    .overflow_hidden()
                    .text_color(rgb(theme::TEXT))
                    .child(pane_header("History").child({
                        let strip = strip.clone();
                        Button::ghost("history-close", "Close").on_click(move |_, _, cx| {
                            strip.update(cx, |strip, cx| strip.dismiss_modal(cx));
                        })
                    }))
                    .child(
                        div()
                            .id("history-body")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.))
                            .gap(px(12.))
                            .overflow_y_scroll()
                            .p(px(16.))
                            .child(
                                div()
                                    .id("history-image")
                                    .debug_selector(|| "history-image".into())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w_full()
                                    .h(px(220.))
                                    .children(
                                        image.map(|image| img(image).max_w_full().max_h_full()),
                                    ),
                            )
                            .children(details.into_iter().map(|line| {
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::TEXT_MUTED))
                                    .child(line)
                            }))
                            .child(div().text_sm().child(prompt))
                            .when(confirming, |body| {
                                body.child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme::TEXT_MUTED))
                                        .child("Discard this result?"),
                                )
                            })
                            .child(div().flex().flex_row().flex_wrap().gap(px(6.)).children(
                                actions.into_iter().map(|label| {
                                    let name = label.clone();
                                    let strip = strip.clone();
                                    div()
                                        .debug_selector(|| format!("history-action-{label}"))
                                        .child(
                                            Button::ghost(format!("history-{name}"), label)
                                                .on_click(move |_, _, cx| {
                                                    strip.update(cx, |strip, cx| {
                                                        strip.press(&name, cx);
                                                    });
                                                }),
                                        )
                                }),
                            )),
                    ),
            )
    }

    fn set_status(&mut self, text: &'static str, cx: &mut Context<Self>) {
        self.notifications
            .update(cx, |notifications, cx| notifications.set_status(text, cx));
    }

    fn open(&mut self, _: &Open, _: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        let state = self.app_state.clone();
        let notes = self.notifications.clone();
        cx.spawn(async move |_root, cx| {
            let picked = receiver.await;
            cx.update(|cx| match picked {
                Ok(Ok(Some(paths))) => {
                    image_load::load_paths(state, notes, paths, image_load::LoadTarget::Paste, cx)
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    notes.update(cx, |notes, cx| {
                        notes.push(
                            crate::notifications::Toast {
                                level: crate::notifications::ToastLevel::Error,
                                text: prompt_failure_text(&error).into(),
                                ttl: std::time::Duration::from_secs(6),
                                action: None,
                            },
                            cx,
                        );
                    });
                }
                Err(_) => {}
            });
        })
        .detach();
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            self.info_toast("Clipboard has no image", cx);
            return;
        };
        let mut image_bytes = None;
        let mut paths = Vec::new();
        for entry in item.entries() {
            match entry {
                gpui::ClipboardEntry::Image(image) => {
                    image_bytes = Some(image.bytes.clone());
                }
                gpui::ClipboardEntry::ExternalPaths(external) => {
                    paths.extend(external.0.iter().cloned());
                }
                gpui::ClipboardEntry::String(text) => {
                    for line in text.text.lines() {
                        let candidate = std::path::PathBuf::from(line.trim());
                        if candidate.is_file() {
                            paths.push(candidate);
                        }
                    }
                }
            }
        }
        if let Some(bytes) = image_bytes {
            image_load::load_bytes(
                self.app_state.clone(),
                self.notifications.clone(),
                bytes,
                "clipboard".into(),
                image_load::LoadTarget::Paste,
                cx,
            );
            return;
        }
        if paths.is_empty() {
            self.info_toast("Clipboard has no image", cx);
            return;
        }
        image_load::load_paths(
            self.app_state.clone(),
            self.notifications.clone(),
            paths,
            image_load::LoadTarget::Paste,
            cx,
        );
    }

    fn info_toast(&mut self, text: &str, cx: &mut Context<Self>) {
        self.notifications.update(cx, |notes, cx| {
            notes.push(
                crate::notifications::Toast {
                    level: crate::notifications::ToastLevel::Info,
                    text: text.to_owned().into(),
                    ttl: std::time::Duration::from_secs(4),
                    action: None,
                },
                cx,
            );
            notes.set_status("Ready", cx);
        });
    }

    fn save_as(&mut self, _: &SaveAs, _: &mut Window, cx: &mut Context<Self>) {
        self.set_status("Save As", cx);
    }

    fn fit(&mut self, _: &Fit, _: &mut Window, cx: &mut Context<Self>) {
        self.canvas.update(cx, |canvas, cx| canvas.fit(cx));
        self.set_status("Fit", cx);
    }

    fn actual_size(&mut self, _: &ActualSize, _: &mut Window, cx: &mut Context<Self>) {
        self.canvas.update(cx, |canvas, cx| canvas.actual_size(cx));
        self.set_status("Actual size", cx);
    }

    fn settings(&mut self, _: &Settings, _: &mut Window, cx: &mut Context<Self>) {
        self.app_state
            .update(cx, |state, cx| state.toggle_settings(cx));
        let label = if self.app_state.read(cx).settings_open {
            "Settings"
        } else {
            "Ready"
        };
        self.set_status(label, cx);
    }

    fn close_settings(&mut self, _: &CloseSettings, _: &mut Window, cx: &mut Context<Self>) {
        if self.app_state.read(cx).settings_open {
            self.app_state
                .update(cx, |state, cx| state.set_settings_open(false, cx));
            self.set_status("Ready", cx);
            return;
        }
        if self.app_state.read(cx).history_item.is_some() {
            self.bottom_strip
                .update(cx, |strip, cx| strip.dismiss_modal(cx));
            self.set_status("Ready", cx);
        }
    }

    #[cfg(test)]
    pub fn strip(&self) -> gpui::Entity<BottomStrip> {
        self.bottom_strip.clone()
    }

    fn quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        self.app_state
            .update(cx, |state, cx| state.request_quit(cx));
        self.set_status("Quit", cx);
        cx.quit();
    }
}

impl Focusable for Root {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for image in self
            .app_state
            .update(cx, |state, _cx| state.take_retired_renders())
        {
            window.drop_image(image).ok();
        }
        let content = div()
            .id("root")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::fit))
            .on_action(cx.listener(Self::actual_size))
            .on_action(cx.listener(Self::settings))
            .on_action(cx.listener(Self::close_settings))
            .on_action(cx.listener(Self::quit))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                image_load::load_paths(
                    this.app_state.clone(),
                    this.notifications.clone(),
                    paths.0.iter().cloned().collect(),
                    image_load::LoadTarget::Paste,
                    cx,
                );
            }))
            .drag_over::<ExternalPaths>(|style, _, _, _| {
                style.border_2().border_color(rgb(theme::ACCENT))
            })
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .font_family(assets::UI_FONT)
            .text_sm()
            .bg(rgb(theme::EDITOR))
            .text_color(rgb(theme::TEXT))
            .when(chrome::is_client_decorated(window), |root| {
                chrome::round_client_corners(root, chrome::tiling(window))
            })
            .child(self.title_bar.clone())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .w_full()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .child(self.side_panel.clone())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .child(self.toolbar.clone())
                            .child(self.canvas.clone()),
                    )
                    .child(self.bottom_strip.clone()),
            )
            .child(self.status_bar.clone());
        let history_dialog = self.bottom_strip.update(cx, |strip, cx| strip.dialog(cx));
        let content = content
            .when_some(history_dialog, |root, dialog| {
                root.child(self.history_modal(dialog))
            })
            .when(self.app_state.read(cx).settings_open, |root| {
                root.child(self.settings_modal(cx))
            });

        let root = cx.entity();
        chrome::client_side_decorations(
            content,
            self.hovered_edge,
            move |edge, _window, cx| {
                root.update(cx, |root, cx| {
                    if root.hovered_edge != edge {
                        root.hovered_edge = edge;
                        cx.notify();
                    }
                });
            },
            window,
        )
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Focusable as _, TestAppContext, VisualTestContext, prelude::*};

    use crate::actions::Quit;

    use super::Root;

    #[gpui::test]
    fn quit_action_sets_the_flag(cx: &mut TestAppContext) {
        let window = cx.update(|cx| {
            cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let root = cx.new(Root::new);
                root.focus_handle(cx).focus(window, cx);
                root
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let root = window.root(&mut cx).unwrap();
        cx.update(|window, cx| {
            window.dispatch_action(Box::new(Quit), cx);
        });
        let quit = root.read_with(&cx, |root, cx| root.app_state.read(cx).quit_requested);
        assert!(quit);
    }

    #[gpui::test]
    fn settings_opens_a_modal_and_escape_closes_it(cx: &mut TestAppContext) {
        let window = cx.update(|cx| {
            crate::actions::bind_keys(cx);
            cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let root = cx.new(Root::new);
                root.focus_handle(cx).focus(window, cx);
                root
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let root = window.root(&mut cx).unwrap();
        cx.update(|window, cx| {
            window.dispatch_action(Box::new(crate::actions::Settings), cx);
        });
        assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).settings_open));
        cx.update(|window, cx| {
            window.dispatch_action(Box::new(crate::actions::CloseSettings), cx);
        });
        assert!(!root.read_with(&cx, |root, cx| root.app_state.read(cx).settings_open));
    }
}
