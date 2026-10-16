use gpui::{IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

use crate::theme;
use crate::ui::section_label;

/// The bottom dock. Reference images land here in a later step; for now it is
/// a labeled, horizontally scrollable row.
pub struct BottomStrip;

impl Render for BottomStrip {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .w_full()
            .h(px(theme::STRIP_HEIGHT))
            .px(px(12.))
            .id("bottom-strip")
            .bg(rgb(theme::PANEL))
            .border_t_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .overflow_x_scroll()
            .child(section_label("References"))
    }
}
