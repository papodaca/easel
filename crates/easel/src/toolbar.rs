use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

use crate::notifications::Notifications;
use crate::theme;

pub struct Toolbar {
    notifications: gpui::Entity<Notifications>,
}

impl Toolbar {
    pub fn new(notifications: gpui::Entity<Notifications>) -> Self {
        Self { notifications }
    }
}

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.notifications.read(cx).status().clone();
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w_full()
            .h(px(theme::TOOLBAR_HEIGHT))
            .bg(rgb(theme::TOOLBAR))
            .text_color(rgb(theme::TEXT))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .h(px(44.))
                    .px(px(8.))
                    .child(tool_button("open", "Open", |window, cx| {
                        window.dispatch_action(Box::new(crate::actions::Open), cx);
                    }))
                    .child(tool_button("paste", "Paste", |window, cx| {
                        window.dispatch_action(Box::new(crate::actions::Paste), cx);
                    }))
                    .child(tool_button("fit", "Fit", |window, cx| {
                        window.dispatch_action(Box::new(crate::actions::Fit), cx);
                    }))
                    .child(tool_button("actual-size", "100%", |window, cx| {
                        window.dispatch_action(Box::new(crate::actions::ActualSize), cx);
                    }))
                    .child(tool_button("settings", "Settings", |window, cx| {
                        window.dispatch_action(Box::new(crate::actions::Settings), cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(28.))
                    .px(px(8.))
                    .text_sm()
                    .text_color(rgb(theme::MUTED))
                    .child(status),
            )
    }
}

fn tool_button(
    id: &'static str,
    label: &'static str,
    action: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(28.))
        .px(px(10.))
        .bg(rgb(theme::BUTTON))
        .hover(|style| style.bg(rgb(theme::BUTTON_HOVER)))
        .text_color(rgb(theme::TEXT))
        .cursor_pointer()
        .on_click(move |_, window, cx| action(window, cx))
        .child(label)
}
