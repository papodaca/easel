use std::sync::Arc;

use easel_core::model_spec::ProviderId;
use gpui::{Context, Entity, IntoElement, ParentElement, Render, Window, div, prelude::*, px, rgb};

use crate::app_state::AppState;
use crate::net::KeyResolver;
use crate::theme;
use crate::ui::section_label;

pub(crate) fn missing_key_line(provider: ProviderId) -> String {
    let name = match provider {
        ProviderId::Bfl => "BFL",
        ProviderId::Krea => "Krea",
        ProviderId::Google => "Google",
        ProviderId::OpenAi => "OpenAI",
        ProviderId::Xai => "xAI",
        ProviderId::Chutes => "Chutes",
        ProviderId::Qwen => "Qwen",
        ProviderId::Apiframe => "Apiframe",
        ProviderId::Leonardo => "Leonardo.Ai",
    };
    format!("No {name} API key — open Settings")
}

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
        let sent = image_count.min(app.max_sent_images());
        let mut messages = app
            .field_errors(sent)
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>();
        let provider = app.spec().provider;
        if self.keys.key_for(provider).is_none() {
            messages.push(missing_key_line(provider));
        }
        if provider == ProviderId::Google
            && let Ok(request) = crate::jobs::build_request(app)
            && easel_core::providers::google::posted_json_len(app.spec(), &request)
                > easel_core::providers::google::BODY_CAP
        {
            messages.push("The request is over 20 MB. Remove a reference.".into());
        }
        if provider == ProviderId::OpenAi {
            let prepared = usize::from(app.image.is_some()) + app.extras.len();
            let (prompt, urls) = match crate::jobs::build_request(app) {
                Ok(request) => {
                    let urls = request
                        .images
                        .iter()
                        .map(|image| {
                            easel_core::providers::openai::data_url(
                                &image.media_type,
                                &image.base64(),
                            )
                        })
                        .collect();
                    (request.compiled_prompt, urls)
                }
                Err(_) => (app.compiled().text, Vec::new()),
            };
            messages.extend(easel_core::providers::openai::refusal_messages(
                &prompt, &urls, prepared,
            ));
        }
        if provider == ProviderId::Xai {
            let prepared = usize::from(app.image.is_some()) + app.extras.len();
            messages.extend(easel_core::providers::xai::refusal_messages(prepared));
        }
        if provider == ProviderId::Qwen {
            let prepared = usize::from(app.image.is_some()) + app.extras.len();
            if image_count > prepared {
                messages.push("A prepared image is missing.".into());
            }
            if let Ok(request) = crate::jobs::build_request(app) {
                messages.extend(easel_core::providers::qwen::refusal_messages(
                    app.spec(),
                    prepared,
                    &request.images,
                ));
            }
        }
        messages
    }

    pub fn send_notice(&self, cx: &gpui::App) -> Option<String> {
        let app = self.app_state.read(cx);
        let prepared = usize::from(app.image.is_some()) + app.extras.len();
        if app.spec().provider == ProviderId::OpenAi
            && prepared > easel_core::providers::openai::IMAGE_CAP
        {
            return None;
        }
        if app.spec().provider == ProviderId::Xai
            && prepared > easel_core::providers::xai::IMAGE_CAP
        {
            return None;
        }
        if app.spec().provider == ProviderId::Qwen
            && prepared > app.spec().capabilities.image_slots.max()
        {
            return None;
        }
        let image_count =
            usize::from(app.scene.source_image.is_some()) + app.scene.references.len();
        let sent = image_count.min(app.max_sent_images());
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

#[cfg(test)]
mod tests {
    use easel_core::model_spec::ProviderId;

    #[test]
    fn missing_key_lines_name_each_provider() {
        assert_eq!(
            super::missing_key_line(ProviderId::Chutes),
            "No Chutes API key — open Settings"
        );
        assert_eq!(
            super::missing_key_line(ProviderId::Xai),
            "No xAI API key — open Settings"
        );
        assert_eq!(
            super::missing_key_line(ProviderId::Qwen),
            "No Qwen API key — open Settings"
        );
        assert_eq!(
            super::missing_key_line(ProviderId::Apiframe),
            "No Apiframe API key — open Settings"
        );
        assert_eq!(
            super::missing_key_line(ProviderId::Leonardo),
            "No Leonardo.Ai API key — open Settings"
        );
    }
}
