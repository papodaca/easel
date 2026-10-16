mod inspector;
mod prompt_editor;
mod references;
mod schema_form;
mod validation;

pub use prompt_editor::PromptEditor;
pub use references::References;
pub use schema_form::SchemaForm;
pub use validation::Validation;

#[cfg(test)]
mod form_tests;

use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

pub use inspector::Inspector;

use crate::actions::Settings;
use crate::app_state::{AppState, SideMode};
use crate::settings::{SettingsPanel, SharedKeys};
use crate::theme;
use crate::ui::{Button, pane_header};

pub struct SidePanel {
    app_state: gpui::Entity<AppState>,
    settings: gpui::Entity<SettingsPanel>,
    inspector: gpui::Entity<Inspector>,
    prompt: gpui::Entity<PromptEditor>,
    references: gpui::Entity<References>,
    form: gpui::Entity<SchemaForm>,
    validation: gpui::Entity<Validation>,
}

impl SidePanel {
    pub fn new(
        app_state: gpui::Entity<AppState>,
        settings: gpui::Entity<SettingsPanel>,
        keys: std::sync::Arc<SharedKeys>,
        cx: &mut Context<Self>,
    ) -> Self {
        let inspector = cx.new(|cx| Inspector::new(app_state.clone(), cx));
        let prompt = cx.new(|cx| PromptEditor::new(app_state.clone(), cx));
        let references = cx.new(|cx| References::new(app_state.clone(), cx));
        let form = cx.new(|cx| SchemaForm::new(app_state.clone(), cx));
        let resolver = std::sync::Arc::new(crate::settings::Resolver(keys));
        let validation = cx.new(|cx| Validation::new(app_state.clone(), resolver, cx));
        Self {
            app_state,
            settings,
            inspector,
            prompt,
            references,
            form,
            validation,
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
            SideMode::Editor => (
                pane_header("Editor"),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(self.inspector.clone())
                    .child(self.references.clone())
                    .child(self.prompt.clone())
                    .child(self.form.clone())
                    .child(self.validation.clone())
                    .child({
                        let enabled = self.validation.read(cx).generate_enabled(cx);
                        Button::filled("generate", "Generate").disabled(!enabled)
                    }),
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
