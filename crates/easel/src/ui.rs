//! Small styled building blocks shared by every region: buttons, pane headers,
//! and the 1 px lines between panes.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, ClickEvent, ElementId, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Transformation, Window, div,
    prelude::FluentBuilder, px, radians, rgb, svg,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// Transparent until hovered. Toolbar and header actions.
    Ghost,
    /// Filled with a border. Primary actions inside forms.
    Filled,
}

pub struct Button {
    id: ElementId,
    label: SharedString,
    style: ButtonStyle,
    on_click: Option<ClickHandler>,
    disabled: bool,
}

impl Button {
    pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label, ButtonStyle::Ghost)
    }

    pub fn filled(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, label, ButtonStyle::Filled)
    }

    fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, style: ButtonStyle) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            style,
            on_click: None,
            disabled: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    /// Dispatch `action` on click. Saves a closure at every call site that only
    /// forwards to an action.
    pub fn action(self, action: impl gpui::Action) -> Self {
        self.on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
    }
}

impl IntoElement for Button {
    type Element = gpui::Stateful<gpui::Div>;

    fn into_element(self) -> Self::Element {
        let mut button = div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(px(22.))
            .px(px(8.))
            .rounded_sm()
            .text_sm()
            .text_color(rgb(if self.disabled {
                crate::theme::TEXT_DISABLED
            } else {
                crate::theme::TEXT
            }))
            .when(!self.disabled, |button| button.cursor_pointer())
            .child(self.label);
        button = match self.style {
            ButtonStyle::Ghost => button
                .hover(|style| style.bg(rgb(crate::theme::GHOST_HOVER)))
                .active(|style| style.bg(rgb(crate::theme::ELEMENT))),
            ButtonStyle::Filled => button
                .bg(rgb(crate::theme::ELEMENT))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .hover(|style| style.bg(rgb(crate::theme::ELEMENT_HOVER)))
                .active(|style| style.bg(rgb(crate::theme::ELEMENT_ACTIVE))),
        };
        if let Some(handler) = self.on_click.filter(|_| !self.disabled) {
            button = button.on_click(move |event, window, cx| handler(event, window, cx));
        }
        button
    }
}

/// The 32 px row at the top of a pane: a title on the left, optional controls
/// on the right, 1 px border underneath. Same height as the toolbar so the
/// lines across panes meet.
pub fn pane_header(title: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .justify_between()
        .w_full()
        .h(px(crate::theme::PANE_HEADER_HEIGHT))
        .px(px(12.))
        .border_b_1()
        .border_color(rgb(crate::theme::BORDER))
        .text_sm()
        .text_color(rgb(crate::theme::TEXT))
        .child(div().child(title.into()))
}

/// Small uppercase-free section label inside a panel body.
pub fn section_label(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_xs()
        .text_color(rgb(crate::theme::TEXT_MUTED))
        .child(text.into())
}

pub fn muted_text(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_sm()
        .text_color(rgb(crate::theme::TEXT_MUTED))
        .child(text.into())
}

/// A 16 px arc that turns while a history row is still running.
pub fn spinner(id: impl Into<ElementId>) -> impl IntoElement {
    svg()
        .path("icons/spinner.svg")
        .size(px(16.))
        .flex_none()
        .text_color(rgb(crate::theme::TEXT_MUTED))
        .with_animation(
            id,
            Animation::new(Duration::from_millis(800)).repeat(),
            |icon, delta| {
                icon.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
}
