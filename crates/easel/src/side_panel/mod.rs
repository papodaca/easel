mod inspector;

use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

pub use inspector::Inspector;

use crate::actions::Settings;
use crate::app_state::{AppState, SideMode};
use crate::settings::SettingsPanel;
use crate::theme;
use crate::ui::{Button, pane_header};

pub struct SidePanel {
    app_state: gpui::Entity<AppState>,
    settings: gpui::Entity<SettingsPanel>,
    inspector: gpui::Entity<Inspector>,
}

impl SidePanel {
    pub fn new(
        app_state: gpui::Entity<AppState>,
        settings: gpui::Entity<SettingsPanel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let inspector = cx.new(|cx| Inspector::new(app_state.clone(), cx));
        Self {
            app_state,
            settings,
            inspector,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn inspector(&self) -> &gpui::Entity<Inspector> {
        &self.inspector
    }
}

impl Render for SidePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.app_state.read(cx).side_mode;
        let (header, body) = match mode {
            SideMode::Editor => (pane_header("Editor"), div().child(self.inspector.clone())),
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
