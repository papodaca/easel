use gpui::{Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px, rgb};

use crate::actions::{ActualSize, Fit, Open, Paste, Settings};
use crate::app_state::{AppState, Tool};
use crate::theme;
use crate::ui::Button;

/// The row above the canvas. Same height as the side panel header so the
/// 1 px lines meet across the split.
pub struct Toolbar {
    app_state: Entity<AppState>,
}

impl Toolbar {
    pub fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn box_tool_enabled(&self, cx: &gpui::App) -> bool {
        self.app_state.read(cx).box_tool_enabled()
    }
}

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let box_enabled = self.app_state.read(cx).box_tool_enabled();
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
            .child(select_button(self.app_state.clone()))
            .child(box_button(self.app_state.clone(), box_enabled))
            .child(divider())
            .child(Button::ghost("fit", "Fit").action(Fit))
            .child(Button::ghost("actual-size", "100%").action(ActualSize))
            .child(div().flex_1())
            .child(Button::ghost("settings", "Settings").action(Settings))
    }
}

fn select_button(state: Entity<AppState>) -> Button {
    Button::ghost("select-tool", "Select").on_click(move |_, _, cx| {
        state.update(cx, |app, cx| {
            app.tool = Tool::Select;
            cx.notify();
        });
    })
}

fn box_button(state: Entity<AppState>, enabled: bool) -> Button {
    Button::ghost("box-tool", "Box")
        .disabled(!enabled)
        .on_click(move |_, _, cx| {
            state.update(cx, |app, cx| {
                if app.box_tool_enabled() {
                    app.tool = Tool::Box;
                }
                cx.notify();
            });
        })
}

fn divider() -> gpui::Div {
    div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(rgb(theme::BORDER))
}
