use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

use crate::actions::Settings;
use crate::app_state::{AppState, SideMode};
use crate::settings::SettingsPanel;
use crate::theme;

pub struct SidePanel {
    app_state: gpui::Entity<AppState>,
    settings: gpui::Entity<SettingsPanel>,
}

impl SidePanel {
    pub fn new(app_state: gpui::Entity<AppState>, settings: gpui::Entity<SettingsPanel>) -> Self {
        Self {
            app_state,
            settings,
        }
    }
}

impl Render for SidePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.app_state.read(cx).side_mode;
        let body = match mode {
            SideMode::Editor => div().flex().flex_col().gap(px(8.)).child("Editor").child(
                div()
                    .text_sm()
                    .text_color(rgb(theme::MUTED))
                    .child("The box tool and parameter form land in a later step."),
            ),
            SideMode::Settings => div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child("Settings")
                .child(self.settings.clone())
                .child(
                    div()
                        .id("settings-back")
                        .flex()
                        .items_center()
                        .h(px(28.))
                        .px(px(10.))
                        .w(px(72.))
                        .bg(rgb(theme::BUTTON))
                        .text_color(rgb(theme::TEXT))
                        .cursor_pointer()
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(Settings), cx);
                        })
                        .child("Back"),
                ),
        };
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(theme::SIDE_PANEL_WIDTH))
            .h_full()
            .bg(rgb(theme::SIDE_PANEL))
            .text_color(rgb(theme::TEXT))
            .p(px(12.))
            .child(body)
    }
}
