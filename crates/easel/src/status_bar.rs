use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

use crate::app_state::AppState;
use crate::chrome;
use crate::notifications::Notifications;
use crate::theme;

/// One line across the bottom of the window: the current operation on the
/// left, the loaded image's pixel size on the right.
pub struct StatusBar {
    notifications: gpui::Entity<Notifications>,
    app_state: gpui::Entity<AppState>,
}

impl StatusBar {
    pub fn new(
        notifications: gpui::Entity<Notifications>,
        app_state: gpui::Entity<AppState>,
    ) -> Self {
        Self {
            notifications,
            app_state,
        }
    }
}

impl Render for StatusBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.notifications.read(cx).status().clone();
        let image_size = self
            .app_state
            .read(cx)
            .image
            .as_ref()
            .map(|image| format!("{} × {}", image.size.w, image.size.h));
        let tiling = chrome::tiling(window);
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .justify_between()
            .w_full()
            .relative()
            .h(px(theme::STATUS_BAR_HEIGHT))
            .px(px(12.))
            .bg(rgb(theme::TITLE_BAR))
            .text_xs()
            .text_color(rgb(theme::TEXT_MUTED))
            .when(chrome::is_client_decorated(window), |bar| {
                chrome::round_client_corners(
                    bar,
                    gpui::Tiling {
                        top: true,
                        ..tiling
                    },
                )
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(theme::WINDOW_BORDER)
                    .bg(rgb(theme::BORDER)),
            )
            .child(status)
            .children(image_size)
    }
}
