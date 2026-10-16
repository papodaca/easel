use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

use crate::actions::Settings;
use crate::app_state::{AppState, SideMode};
use crate::settings::SettingsPanel;
use crate::theme;
use crate::ui::{Button, muted_text, pane_header};

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
        let (header, body) = match mode {
            SideMode::Editor => (
                pane_header("Editor"),
                div().child(muted_text(
                    "The box tool and parameter form land in a later step.",
                )),
            ),
            SideMode::Settings => (
                pane_header("Settings")
                    .child(Button::ghost("settings-back", "Back").action(Settings)),
                div().child(self.settings.clone()),
            ),
        };
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(theme::SIDE_PANEL_WIDTH))
            .h_full()
            .bg(rgb(theme::PANEL))
            .border_l_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .child(header)
            .child(
                div()
                    .id("side-panel-body")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(12.))
                    .child(body),
            )
    }
}
