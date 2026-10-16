use std::sync::Arc;

use gpui::{Context, Entity, IntoElement, ParentElement, Render, Window, div, prelude::*, px};

use crate::app_state::AppState;
use crate::image_load::DecodedImage;
use crate::ui::{muted_text, section_label};

pub struct References {
    app_state: Entity<AppState>,
    refusal: Option<String>,
}

impl References {
    pub fn new(app_state: Entity<AppState>, _cx: &mut Context<Self>) -> Self {
        Self {
            app_state,
            refusal: None,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add_decoded(&mut self, image: Arc<DecodedImage>, cx: &mut Context<Self>) {
        let result = self
            .app_state
            .update(cx, |app, cx| app.add_extra(image, cx));
        self.refusal = result.err().map(str::to_string);
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remove_slot(&mut self, index: usize, cx: &mut Context<Self>) {
        self.app_state.update(cx, |app, cx| {
            app.scene.remove_reference(index);
            cx.notify();
        });
        self.refusal = None;
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn refusal(&self) -> Option<String> {
        self.refusal.clone()
    }
}

impl Render for References {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app_state.read(cx);
        let mut labels = Vec::new();
        if app.scene.source_image.is_some() {
            labels.push("ref_image_0".to_string());
        }
        for index in 0..app.scene.references.len() {
            labels.push(format!(
                "ref_image_{}",
                index + usize::from(app.scene.source_image.is_some())
            ));
        }
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(section_label("References"));
        for label in labels {
            column = column.child(muted_text(label));
        }
        if let Some(refusal) = &self.refusal {
            column = column.child(muted_text(refusal.clone()));
        }
        column
    }
}
