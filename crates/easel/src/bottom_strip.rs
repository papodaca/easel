use gpui::{IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

use crate::theme;

pub struct BottomStrip;

impl Render for BottomStrip {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .flex_shrink_0()
            .items_center()
            .w_full()
            .h(px(theme::STRIP_HEIGHT))
            .px(px(12.))
            .id("bottom-strip")
            .bg(rgb(theme::STRIP))
            .text_color(rgb(theme::TEXT))
            .overflow_x_scroll()
            .child("References")
    }
}
