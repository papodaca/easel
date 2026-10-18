use std::sync::Arc;

use gpui::{Context, Entity, IntoElement, ParentElement, Render, Window, div, prelude::*, px};

use crate::app_state::AppState;
use crate::image_load::DecodedImage;
use crate::ui::{muted_text, section_label};

pub struct References {
    app_state: Entity<AppState>,
    refusal: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
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
        self.refusal = result.err();
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remove_slot(&mut self, index: usize, cx: &mut Context<Self>) {
        self.app_state
            .update(cx, |app, cx| app.remove_reference_slot(index, cx));
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
        let labels = self
            .app_state
            .read(cx)
            .reference_marks()
            .into_iter()
            .map(|(label, _)| label)
            .collect::<Vec<_>>();
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
