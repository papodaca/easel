use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, px, rgb};

use crate::actions::{ActualSize, Fit, Open, Paste, Settings};
use crate::theme;
use crate::ui::Button;

/// The row above the canvas. Same height as the side panel header so the
/// 1 px lines meet across the split.
pub struct Toolbar;

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .w_full()
            .h(px(theme::PANE_HEADER_HEIGHT))
            .px(px(6.))
            .bg(rgb(theme::PANEL))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .child(Button::ghost("open", "Open").action(Open))
            .child(Button::ghost("paste", "Paste").action(Paste))
            .child(divider())
            .child(Button::ghost("fit", "Fit").action(Fit))
            .child(Button::ghost("actual-size", "100%").action(ActualSize))
            .child(div().flex_1())
            .child(Button::ghost("settings", "Settings").action(Settings))
    }
}

fn divider() -> gpui::Div {
    div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(rgb(theme::BORDER))
}
