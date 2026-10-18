mod inspector;
mod prompt_editor;
mod references;
mod schema_form;
mod validation;

pub use prompt_editor::PromptEditor;
#[allow(unused_imports)]
pub use references::References;
pub use schema_form::SchemaForm;
pub use validation::Validation;

#[cfg(test)]
mod form_tests;
#[cfg(test)]
mod generate_tests;
#[cfg(test)]
mod switch_tests;

use gpui::{
    Bounds, Context, IntoElement, MouseButton, ParentElement, Pixels, Render, ScrollHandle, Styled,
    Window, canvas, div, point, prelude::*, px, quad, rgb, size,
};

pub use inspector::Inspector;

use crate::app_state::AppState;
use crate::net::KeyResolver;
use crate::settings::{Resolver, SettingsPanel, SharedKeys};
use crate::theme;
use crate::ui::{Button, pane_header};
use crate::widgets::Select;

fn apply_provider(state: &gpui::Entity<AppState>, name: &str, cx: &mut gpui::App) {
    let Some(provider) = easel_core::providers::ModelRegistry::providers()
        .iter()
        .copied()
        .find(|provider| provider.display_name() == name)
    else {
        return;
    };
    state.update(cx, |app, cx| {
        let Some(id) = app.model_for_provider(provider) else {
            return;
        };
        app.set_model(&id, cx);
    });
}

fn apply_model(state: &gpui::Entity<AppState>, name: &str, cx: &mut gpui::App) {
    let provider = state.read(cx).spec().provider;
    let Some(spec) = easel_core::providers::ModelRegistry::specs()
        .iter()
        .find(|spec| spec.provider == provider && spec.display_name == name)
    else {
        return;
    };
    let id = spec.id.clone();
    state.update(cx, |app, cx| app.set_model(&id, cx));
}

pub struct SidePanel {
    app_state: gpui::Entity<AppState>,
    inspector: gpui::Entity<Inspector>,
    prompt: gpui::Entity<PromptEditor>,
    form: gpui::Entity<SchemaForm>,
    validation: gpui::Entity<Validation>,
    keys: std::sync::Arc<SharedKeys>,
    provider: gpui::Entity<Select>,
    model: gpui::Entity<Select>,
    runner: Option<gpui::Entity<crate::jobs::JobRunner>>,
    scroll: ScrollHandle,
    thumb: Bounds<Pixels>,
    thumb_grab: Option<Pixels>,
}

impl SidePanel {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn shows_reference_section(&self) -> bool {
        false
    }

    pub fn new(
        app_state: gpui::Entity<AppState>,
        keys: std::sync::Arc<SharedKeys>,
        runner: Option<gpui::Entity<crate::jobs::JobRunner>>,
        settings: Option<gpui::Entity<SettingsPanel>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let inspector = cx.new(|cx| Inspector::new(app_state.clone(), cx));
        let prompt = cx.new(|cx| PromptEditor::new(app_state.clone(), cx));
        let form = cx.new(|cx| SchemaForm::new(app_state.clone(), cx));
        let resolver = std::sync::Arc::new(Resolver(std::sync::Arc::clone(&keys)));
        let validation = cx.new(|cx| Validation::new(app_state.clone(), resolver, cx));
        let provider = cx.new(|cx| {
            let mut select = Select::new(Vec::new(), cx);
            let state = app_state.clone();
            select.on_change(
                Box::new(move |value, _window, cx| {
                    let Some(name) = value.as_str() else {
                        return;
                    };
                    apply_provider(&state, name, cx);
                }),
                cx,
            );
            select
        });
        let model = cx.new(|cx| {
            let mut select = Select::new(Vec::new(), cx);
            let state = app_state.clone();
            select.on_change(
                Box::new(move |value, _window, cx| {
                    let Some(name) = value.as_str() else {
                        return;
                    };
                    apply_model(&state, name, cx);
                }),
                cx,
            );
            select
        });
        let mut panel = Self {
            app_state,
            inspector,
            prompt,
            form,
            validation,
            keys,
            provider,
            model,
            runner,
            scroll: ScrollHandle::new(),
            thumb: Bounds::default(),
            thumb_grab: None,
        };
        panel.sync_menus(cx);
        let watched = panel.app_state.clone();
        cx.observe(&watched, |this, _, cx| this.sync_menus(cx))
            .detach();
        if let Some(settings) = settings {
            cx.observe(&settings, |this, _, cx| this.sync_menus(cx))
                .detach();
        }
        panel
    }

    fn sync_menus(&mut self, cx: &mut Context<Self>) {
        let spec = self.app_state.read(cx).spec();
        let provider_name = spec.provider.display_name().to_owned();
        let model_name = spec.display_name.clone();
        let provider = spec.provider;
        let models: Vec<gpui::SharedString> = easel_core::providers::ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == provider)
            .map(|spec| gpui::SharedString::from(spec.display_name.clone()))
            .collect();
        let resolver = Resolver(std::sync::Arc::clone(&self.keys));
        let providers: Vec<gpui::SharedString> = easel_core::providers::ModelRegistry::providers()
            .iter()
            .copied()
            .filter(|provider| resolver.key_for(*provider).is_some())
            .map(|provider| gpui::SharedString::from(provider.display_name()))
            .collect();
        self.provider.update(cx, |select, cx| {
            select.set_options(providers, cx);
            select.set_value(serde_json::Value::String(provider_name), cx);
        });
        self.model.update(cx, |select, cx| {
            select.set_options(models, cx);
            select.set_value(serde_json::Value::String(model_name), cx);
        });
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn choose_provider(&mut self, name: &str, cx: &mut Context<Self>) {
        apply_provider(&self.app_state, name, cx);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn choose_model(&mut self, name: &str, cx: &mut Context<Self>) {
        apply_model(&self.app_state, name, cx);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn provider_menu(&self) -> &gpui::Entity<Select> {
        &self.provider
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn model_menu(&self) -> &gpui::Entity<Select> {
        &self.model
    }

    fn scroll_by_thumb(&mut self, y: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        let Some(grab) = self.thumb_grab else {
            return;
        };
        let track = self.scroll.bounds();
        let max_y = self.scroll.max_offset().y;
        if max_y <= px(0.) {
            return;
        }
        let viewport = track.size.height;
        let content = viewport + max_y;
        let inset = px(2.);
        let track_h = (viewport - inset * 2.).max(px(0.));
        let thumb_h = (viewport / content * track_h).clamp(px(18.), track_h);
        let travel = (track_h - thumb_h).max(px(1.));
        let top = (y - track.top() - inset - grab).clamp(px(0.), travel);
        let progress = top / travel;
        self.scroll.set_offset(point(px(0.), -max_y * progress));
        window.refresh();
        cx.notify();
    }

    pub fn generate(&mut self, cx: &mut Context<Self>) {
        let Some(runner) = self.runner.clone() else {
            return;
        };
        if !self.validation.read(cx).generate_enabled(cx) {
            return;
        }
        let request = match crate::jobs::build_request(self.app_state.read(cx)) {
            Ok(request) => request,
            Err(message) => {
                log::warn!("generate refused: {message}");
                return;
            }
        };
        runner.update(cx, |runner, cx| runner.submit(request, cx));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn inspector(&self) -> &gpui::Entity<Inspector> {
        &self.inspector
    }
}

impl Render for SidePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inspection = self.app_state.read(cx).inspection.clone();
        let (header, body) = if let Some(inspection) = inspection {
            let state = self.app_state.clone();
            (
                pane_header("Compare").child(Button::ghost("compare-return", "Return").on_click(
                    move |_, _, cx| {
                        state.update(cx, |app, cx| {
                            app.inspection = None;
                            cx.notify();
                        });
                    },
                )),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(div().text_sm().child(inspection.prompt.clone()))
                    .when(!inspection.suggestion.is_empty(), |body| {
                        body.child(div().text_sm().child(inspection.suggestion.clone()))
                    })
                    .children(inspection.rows.iter().map(|(key, value)| {
                        div().flex().flex_col().gap(px(2.)).child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::TEXT_MUTED))
                                .child(format!("{key}: {value}")),
                        )
                    })),
            )
        } else {
            (
                pane_header("Editor").child(self.provider.clone()),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(self.model.clone())
                    .child(self.inspector.clone())
                    .child(self.prompt.clone())
                    .child(self.form.clone())
                    .child(self.validation.clone())
                    .child({
                        let enabled = self.validation.read(cx).generate_enabled(cx);
                        let panel = cx.entity();
                        Button::filled("generate", "Generate")
                            .disabled(!enabled)
                            .on_click(move |_, _, cx| {
                                panel.update(cx, |panel, cx| panel.generate(cx));
                            })
                    }),
            )
        };
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(theme::SIDE_PANEL_WIDTH))
            .h_full()
            .bg(rgb(theme::PANEL))
            .border_r_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .child(header)
            .child({
                let scroll = self.scroll.clone();
                let panel = cx.entity();
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .child(
                        div()
                            .id("side-panel-body")
                            .size_full()
                            .overflow_y_scroll()
                            .scrollbar_width(px(10.))
                            .track_scroll(&self.scroll)
                            .p(px(12.))
                            .child(body),
                    )
                    .child(
                        div()
                            .id("editor-scrollbar")
                            .absolute()
                            .top_0()
                            .right_0()
                            .h_full()
                            .w(px(10.))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                                    if this.thumb.contains(&event.position) {
                                        this.thumb_grab = Some(event.position.y - this.thumb.top());
                                    } else if this.scroll.max_offset().y > px(0.) {
                                        this.thumb_grab = Some(this.thumb.size.height / 2.);
                                        this.scroll_by_thumb(event.position.y, window, cx);
                                    }
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                |this, event: &gpui::MouseMoveEvent, window, cx| {
                                    this.scroll_by_thumb(event.position.y, window, cx);
                                },
                            ))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.thumb_grab = None;
                                    cx.notify();
                                }),
                            )
                            .on_mouse_up_out(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.thumb_grab = None;
                                    cx.notify();
                                }),
                            )
                            .child({
                                let paint_scroll = scroll.clone();
                                canvas(
                                    move |_bounds, _, cx| {
                                        let thumb = editor_thumb(&scroll);
                                        panel.update(cx, |panel, _| {
                                            panel.thumb = thumb.unwrap_or_default();
                                        });
                                    },
                                    move |_bounds, _, window, _| {
                                        let Some(thumb) = editor_thumb(&paint_scroll) else {
                                            return;
                                        };
                                        let track = paint_scroll.bounds();
                                        window.paint_quad(quad(
                                            Bounds {
                                                origin: point(track.right() - px(10.), track.top()),
                                                size: size(px(10.), track.size.height),
                                            },
                                            px(0.),
                                            theme::color(theme::PANEL, 1.),
                                            px(0.),
                                            theme::color(theme::PANEL, 0.),
                                            gpui::BorderStyle::Solid,
                                        ));
                                        window.paint_quad(quad(
                                            thumb,
                                            px(3.),
                                            theme::color(theme::TEXT, 0.55),
                                            px(0.),
                                            theme::color(theme::TEXT, 0.),
                                            gpui::BorderStyle::Solid,
                                        ));
                                    },
                                )
                                .size_full()
                            }),
                    )
            })
    }
}

fn editor_thumb(scroll: &ScrollHandle) -> Option<Bounds<Pixels>> {
    let track = scroll.bounds();
    let max_y = scroll.max_offset().y;
    if max_y <= px(1.) || track.size.height <= px(1.) {
        return None;
    }
    let viewport = track.size.height;
    let content = viewport + max_y;
    let inset = px(3.);
    let track_h = (viewport - inset * 2.).max(px(0.));
    let thumb_h = (viewport / content * track_h).clamp(px(28.), track_h);
    let travel = (track_h - thumb_h).max(px(0.));
    let progress = (-scroll.offset().y / max_y).clamp(0., 1.);
    Some(Bounds {
        origin: point(
            track.right() - px(8.),
            track.top() + inset + travel * progress,
        ),
        size: size(px(6.), thumb_h),
    })
}
