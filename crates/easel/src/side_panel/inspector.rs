use easel_core::scene::{BoxId, BoxKind};
use gpui::{
    Context, Entity, Focusable, IntoElement, ParentElement, Render, Subscription, Window, div,
    prelude::*, px,
};

use crate::app_state::AppState;
use crate::theme;
use crate::ui::Button;
use crate::widgets::{Select, TextInput};

pub struct Inspector {
    app_state: Entity<AppState>,
    id_input: Entity<TextInput>,
    kind: Entity<Select>,
    desc: Entity<TextInput>,
    from_image: Entity<Select>,
    bound: Option<BoxId>,
    _observe: Subscription,
}

impl Inspector {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let id_input = cx.new(|cx| {
            let mut input = TextInput::new(cx);
            input.set_label("Id", cx);
            input
        });
        let kind = cx.new(|cx| {
            let mut select = Select::new(
                ["Layout", "New", "Keep", "Move", "Remove"]
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                cx,
            );
            select.set_label("Kind", cx);
            select
        });
        let desc = cx.new(|cx| {
            let mut input = TextInput::new(cx);
            input.set_label("Description", cx);
            input
        });
        let from_image = cx.new(|cx| {
            let mut select = Select::new(vec!["none".into(), "ref_image_0".into()], cx);
            select.set_label("From", cx);
            select
        });
        let observe_state = app_state.clone();
        let observe = cx.observe(&observe_state, |_, _, cx| cx.notify());
        let inspector = Self {
            app_state,
            id_input,
            kind,
            desc,
            from_image,
            bound: None,
            _observe: observe,
        };
        inspector.wire(cx);
        inspector
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn shows_box(&self) -> bool {
        self.bound.is_some()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn id_text(&self, cx: &gpui::App) -> String {
        self.id_input.read(cx).text().to_string()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_kind(&mut self, kind: BoxKind, cx: &mut Context<Self>) {
        self.apply_kind(kind, cx);
    }

    fn wire(&self, cx: &mut Context<Self>) {
        let state = self.app_state.clone();
        self.id_input.update(cx, |input, _cx| {
            input.on_submit_callback(Box::new(move |text, _window, cx| {
                let text = text.to_string();
                state.update(cx, |app, cx| commit_id(app, &text, cx));
            }));
        });
        let state = self.app_state.clone();
        self.kind.update(cx, |select, cx| {
            select.on_change(
                Box::new(move |value, _window, cx| {
                    let Some(name) = value.as_str() else {
                        return;
                    };
                    let Some(kind) = parse_kind(name) else {
                        return;
                    };
                    state.update(cx, |app, cx| {
                        if let Some(id) = app.selected_box.clone() {
                            begin_kind(app, &id, kind);
                            cx.notify();
                        }
                    });
                }),
                cx,
            );
        });
        let state = self.app_state.clone();
        self.desc.update(cx, |input, cx| {
            input.on_change(
                Box::new(move |value, _window, cx| {
                    let Some(text) = value.as_str() else {
                        return;
                    };
                    let text = text.to_string();
                    state.update(cx, |app, cx| {
                        if let Some(id) = app.selected_box.clone() {
                            app.scene.update_box(&id, |element| element.desc = text);
                            cx.notify();
                        }
                    });
                }),
                cx,
            );
        });
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn apply_kind(&mut self, kind: BoxKind, cx: &mut Context<Self>) {
        self.app_state.update(cx, |app, cx| {
            if let Some(id) = app.selected_box.clone() {
                begin_kind(app, &id, kind);
                cx.notify();
            }
        });
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.app_state.read(cx).selected_box.clone();
        let focus = self.app_state.read(cx).focus_box_field;
        if selected.as_ref().map(BoxId::as_str) != self.bound.as_ref().map(BoxId::as_str) {
            self.bound = selected.clone();
            if let Some(id) = &selected {
                let (text, kind, desc) = {
                    let app = self.app_state.read(cx);
                    let element = app.scene.boxes.iter().find(|element| &element.id == id);
                    (
                        id.as_str().to_string(),
                        element
                            .map(|element| kind_name(element.kind))
                            .unwrap_or("New"),
                        element
                            .map(|element| element.desc.clone())
                            .unwrap_or_default(),
                    )
                };
                self.id_input
                    .update(cx, |input, cx| input.set_text(&text, cx));
                self.kind.update(cx, |select, cx| {
                    select.set_value(serde_json::Value::String(kind.into()), cx);
                });
                self.desc.update(cx, |input, cx| input.set_text(&desc, cx));
            }
        }
        if focus {
            self.app_state
                .update(cx, |app, _| app.focus_box_field = false);
            let input = self.id_input.clone();
            cx.defer_in(window, move |_inspector, window, cx| {
                input.update(cx, |input, cx| {
                    input.select_all_text(cx);
                    input.focus_handle(cx).focus(window, cx);
                });
            });
        }
    }
}

fn commit_id(app: &mut AppState, text: &str, cx: &mut Context<AppState>) {
    let Some(id) = app.selected_box.clone() else {
        return;
    };
    let next = if easel_core::scene::BoxId::parse(text).is_ok() {
        text.to_string()
    } else if is_prefix(text) {
        app.scene.next_id(text).as_str().to_string()
    } else {
        return;
    };
    let _ = app.scene.rename_box(&id, &next);
    if let Ok(renamed) = BoxId::parse(&next) {
        app.selected_box = Some(renamed);
    }
    cx.notify();
}

fn begin_kind(app: &mut AppState, id: &BoxId, kind: BoxKind) {
    let allowed = app.scene.allowed_kinds().contains(&kind);
    if !allowed {
        return;
    }
    let Some(element) = app.scene.boxes.iter_mut().find(|element| &element.id == id) else {
        return;
    };
    if kind == BoxKind::Move {
        app.revert_kind = Some(element.kind);
        app.saved_src = element.src;
        app.saved_tgt = element.tgt;
        if element.src.is_none() {
            element.src = element.tgt.take();
        }
        element.kind = BoxKind::Move;
        element.from_image.get_or_insert(0);
        if element.tgt.is_none() {
            app.draw_target = Some(id.clone());
        }
        return;
    }
    element.kind = kind;
    app.draw_target = None;
}

fn is_prefix(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase() && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

fn parse_kind(name: &str) -> Option<BoxKind> {
    Some(match name {
        "Layout" => BoxKind::Layout,
        "New" => BoxKind::New,
        "Keep" => BoxKind::Keep,
        "Move" => BoxKind::Move,
        "Remove" => BoxKind::Remove,
        _ => return None,
    })
}

fn kind_name(kind: BoxKind) -> &'static str {
    match kind {
        BoxKind::Layout => "Layout",
        BoxKind::New => "New",
        BoxKind::Keep => "Keep",
        BoxKind::Move => "Move",
        BoxKind::Remove => "Remove",
    }
}

impl Render for Inspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(window, cx);
        if self.bound.is_none() {
            return div();
        }
        let state = self.app_state.clone();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(8.))
            .p(px(8.))
            .rounded_md()
            .bg(gpui::rgb(theme::SURFACE))
            .border_1()
            .border_color(gpui::rgb(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("Box"))
                    .child(
                        Button::ghost("box-remove", "Remove").on_click(move |_, _, cx| {
                            state.update(cx, |app, cx| {
                                if let Some(id) = app.selected_box.take() {
                                    app.scene.remove_box(&id);
                                    app.draw_target = None;
                                    cx.notify();
                                }
                            });
                        }),
                    ),
            )
            .child(self.id_input.clone())
            .child(self.kind.clone())
            .child(self.desc.clone())
            .child(self.from_image.clone())
    }
}
