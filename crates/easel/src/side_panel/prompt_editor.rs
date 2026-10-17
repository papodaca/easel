use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Subscription, Window, div, prelude::*, px,
};

use crate::app_state::AppState;
use crate::ui::{muted_text, section_label};
use crate::widgets::{TextArea, TextInput};

pub struct PromptEditor {
    app_state: Entity<AppState>,
    input: Entity<TextInput>,
    _observe: Subscription,
}

impl PromptEditor {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let prose = app_state.read(cx).scene.prose.clone();
        let input = cx.new(|cx| {
            let mut input = TextArea::new(cx);
            input.set_text(&prose, cx);
            input
        });
        let state = app_state.clone();
        input.update(cx, |input, cx| {
            input.on_change(
                Box::new(move |value, _window, cx| {
                    let Some(text) = value.as_str() else {
                        return;
                    };
                    let text = text.to_string();
                    state.update(cx, |app, cx| {
                        app.scene.prose = text;
                        cx.notify();
                    });
                }),
                cx,
            );
        });
        Self {
            app_state: app_state.clone(),
            input,
            _observe: cx.observe(&app_state, |_, _, cx| cx.notify()),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn prose(&self, cx: &gpui::App) -> String {
        self.input.read(cx).text().to_string()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_cursor(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_cursor(offset, cx));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn insert_tag(&mut self, name: &str, cx: &mut Context<Self>) {
        let tag = format!("<{name}>");
        self.input
            .update(cx, |input, cx| input.insert_at_cursor(&tag, cx));
        let text = self.input.read(cx).text().to_string();
        self.app_state.update(cx, |app, cx| {
            app.scene.prose = text;
            cx.notify();
        });
    }

    fn preview(&self, cx: &gpui::App) -> String {
        let compiled = self.app_state.read(cx).compiled();
        if compiled.warnings.is_empty() {
            compiled.text
        } else {
            format!("{}\n{}", compiled.text, compiled.warnings.join("\n"))
        }
    }
}

impl Render for PromptEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = self.preview(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(section_label("Prompt"))
            .child(self.input.clone())
            .child(section_label("Compiled"))
            .child(muted_text(preview))
    }
}
