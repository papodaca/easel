use std::sync::Arc;

use easel_core::model_spec::ProviderId;
use easel_core::providers::Provider;
use easel_core::providers::bfl::flux3::Flux3;
use gpui::{Context, Entity, IntoElement, ParentElement, Render, Window, div, prelude::*, px, rgb};

use crate::app_state::AppState;
use crate::net::KeyResolver;
use crate::theme;
use crate::ui::section_label;

pub struct Validation {
    app_state: Entity<AppState>,
    keys: Arc<dyn KeyResolver>,
}

impl Validation {
    pub fn new(
        app_state: Entity<AppState>,
        keys: Arc<dyn KeyResolver>,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self { app_state, keys }
    }

    pub fn messages(&self, cx: &gpui::App) -> Vec<String> {
        let app = self.app_state.read(cx);
        let image_count =
            usize::from(app.scene.source_image.is_some()) + app.scene.references.len();
        let mut messages = Flux3
            .validate_for_submit(&app.scene, &app.values, image_count)
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>();
        if self.keys.key_for(ProviderId::Bfl).is_none() {
            messages.push("No BFL API key — open Settings".into());
        }
        messages
    }

    pub fn generate_enabled(&self, cx: &gpui::App) -> bool {
        self.messages(cx).is_empty()
    }
}

impl Render for Validation {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let messages = self.messages(cx);
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(section_label("Validation"));
        for message in messages {
            column = column.child(div().text_xs().text_color(rgb(theme::ERROR)).child(message));
        }
        column
    }
}
