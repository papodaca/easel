use gpui::{
    App, Context, ExternalPaths, FocusHandle, Focusable, IntoElement, ParentElement,
    PathPromptOptions, Render, Styled, Window, div, prelude::*, px, rgb,
};

use crate::actions::{ActualSize, Fit, Open, Paste, Quit, SaveAs, Settings};
use crate::app_state::AppState;
use crate::bottom_strip::BottomStrip;
use crate::canvas::Canvas;
use crate::image_load::{self, prompt_failure_text};
use crate::notifications::Notifications;
use crate::side_panel::SidePanel;
use crate::theme;
use crate::toolbar::Toolbar;

pub struct Root {
    focus_handle: FocusHandle,
    pub app_state: gpui::Entity<AppState>,
    notifications: gpui::Entity<Notifications>,
    toolbar: gpui::Entity<Toolbar>,
    canvas: gpui::Entity<Canvas>,
    side_panel: gpui::Entity<SidePanel>,
    bottom_strip: gpui::Entity<BottomStrip>,
}

impl Root {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let app_state = cx.new(|_cx| AppState::new());
        let notifications = cx.new(|_cx| Notifications::new());
        let toolbar = cx.new(|_cx| Toolbar::new(notifications.clone()));
        let canvas = cx.new(|cx| Canvas::new(app_state.clone(), notifications.clone(), cx));
        let side_panel = cx.new(|_cx| SidePanel::new(app_state.clone()));
        let bottom_strip = cx.new(|_cx| BottomStrip);
        Self {
            focus_handle: cx.focus_handle(),
            app_state,
            notifications,
            toolbar,
            canvas,
            side_panel,
            bottom_strip,
        }
    }

    fn set_status(&mut self, text: &'static str, cx: &mut Context<Self>) {
        self.notifications
            .update(cx, |notifications, cx| notifications.set_status(text, cx));
    }

    fn open(&mut self, _: &Open, _: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let state = self.app_state.clone();
        let notes = self.notifications.clone();
        cx.spawn(async move |_root, cx| {
            let picked = receiver.await;
            cx.update(|cx| match picked {
                Ok(Ok(Some(paths))) => image_load::load_paths(state, notes, paths, cx),
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
            .update(cx, |state, cx| state.toggle_side_mode(cx));
        let mode = self.app_state.read(cx).side_mode;
        let label = match mode {
            crate::app_state::SideMode::Settings => "Settings",
            crate::app_state::SideMode::Editor => "Editor",
        };
        self.set_status(label, cx);
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
        div()
            .id("root")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::fit))
            .on_action(cx.listener(Self::actual_size))
            .on_action(cx.listener(Self::settings))
            .on_action(cx.listener(Self::quit))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                image_load::load_paths(
                    this.app_state.clone(),
                    this.notifications.clone(),
                    paths.0.iter().cloned().collect(),
                    cx,
                );
            }))
            .drag_over::<ExternalPaths>(|style, _, _, _| {
                style.border_2().border_color(rgb(theme::BUTTON_HOVER))
            })
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme::WINDOW))
            .text_color(rgb(theme::TEXT))
            .child(self.toolbar.clone())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .w_full()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .child(self.canvas.clone())
                    .child(self.side_panel.clone()),
            )
            .child(self.bottom_strip.clone())
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
}
