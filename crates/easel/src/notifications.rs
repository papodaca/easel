use std::time::Duration;

use gpui::{
    Action, Context, EventEmitter, IntoElement, ParentElement, Render, SharedString, Window, div,
    prelude::*, px, rgb,
};

use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Error,
}

pub struct Toast {
    pub level: ToastLevel,
    pub text: SharedString,
    pub ttl: Duration,
    pub action: Option<(SharedString, Box<dyn Action>)>,
}

struct LiveToast {
    id: u64,
    level: ToastLevel,
    text: SharedString,
    action: Option<(SharedString, Box<dyn Action>)>,
}

pub struct Notifications {
    toasts: Vec<LiveToast>,
    next_id: u64,
    status: SharedString,
}

impl Notifications {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 1,
            status: "Ready".into(),
        }
    }

    pub fn status(&self) -> &SharedString {
        &self.status
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn toast_count(&self) -> usize {
        self.toasts.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn last_toast_is_error(&self) -> bool {
        self.toasts
            .last()
            .is_some_and(|toast| toast.level == ToastLevel::Error)
    }

    pub fn set_status(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status = text.into();
        cx.notify();
    }

    pub fn push(&mut self, toast: Toast, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        let ttl = toast.ttl;
        self.toasts.push(LiveToast {
            id,
            level: toast.level,
            text: toast.text,
            action: toast.action,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ttl).await;
            this.update(cx, |this, cx| this.dismiss(id, cx)).ok();
        })
        .detach();
    }

    fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        let before = self.toasts.len();
        self.toasts.retain(|toast| toast.id != id);
        if self.toasts.len() != before {
            cx.notify();
        }
    }
}

impl EventEmitter<()> for Notifications {}

impl Render for Notifications {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toasts = self
            .toasts
            .iter()
            .map(|toast| toast_view(toast, cx))
            .collect::<Vec<_>>();
        div()
            .absolute()
            .top(px(8.))
            .right(px(8.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(toasts)
    }
}

fn toast_view(toast: &LiveToast, cx: &mut Context<Notifications>) -> gpui::AnyElement {
    let id = toast.id;
    let background = match toast.level {
        ToastLevel::Info => theme::TOAST_INFO,
        ToastLevel::Error => theme::TOAST_ERROR,
    };
    let mut body = div()
        .id(("toast", id))
        .flex()
        .flex_col()
        .gap(px(6.))
        .w(px(280.))
        .p(px(8.))
        .bg(rgb(background))
        .text_color(rgb(theme::TEXT))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| this.dismiss(id, cx)))
        .child(toast.text.clone());
    if let Some((label, action)) = &toast.action {
        let action = action.boxed_clone();
        body = body.child(
            div()
                .id(("toast-action", id))
                .px(px(8.))
                .py(px(4.))
                .bg(rgb(theme::BUTTON))
                .text_color(rgb(theme::TEXT))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    window.dispatch_action(action.boxed_clone(), cx);
                    this.dismiss(id, cx);
                    cx.stop_propagation();
                }))
                .child(label.clone()),
        );
    }
    body.into_any_element()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{TestAppContext, prelude::*};

    use super::{Notifications, Toast, ToastLevel};

    #[gpui::test]
    async fn toast_expires_after_its_ttl(cx: &mut TestAppContext) {
        let notes = cx.new(|_| Notifications::new());
        notes.update(cx, |notes, cx| {
            notes.push(
                Toast {
                    level: ToastLevel::Info,
                    text: "saved".into(),
                    ttl: Duration::from_secs(2),
                    action: None,
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(notes.read_with(cx, |notes, _| notes.toast_count()), 1);

        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(notes.read_with(cx, |notes, _| notes.toast_count()), 1);

        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(notes.read_with(cx, |notes, _| notes.toast_count()), 0);
    }
}
