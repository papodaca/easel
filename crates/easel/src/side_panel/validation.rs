use std::sync::Arc;

use easel_core::model_spec::ProviderId;
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
        let sent = image_count.min(app.spec().capabilities.image_slots.max());
        let mut messages = app
            .field_errors(sent)
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>();
        let provider = app.spec().provider;
        if self.keys.key_for(provider).is_none() {
            let name = match provider {
                ProviderId::Bfl => "BFL",
                ProviderId::Krea => "Krea",
            };
            messages.push(format!("No {name} API key — open Settings"));
        }
        messages
    }

    pub fn send_notice(&self, cx: &gpui::App) -> Option<String> {
        let app = self.app_state.read(cx);
        let image_count =
            usize::from(app.scene.source_image.is_some()) + app.scene.references.len();
        let sent = image_count.min(app.spec().capabilities.image_slots.max());
        (image_count > sent).then(|| format!("{sent} of {image_count} images will be sent"))
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
        if let Some(notice) = self.send_notice(cx) {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child(notice),
            );
        }
        column
    }
}
