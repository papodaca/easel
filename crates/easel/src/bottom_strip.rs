use std::collections::HashMap;
use std::sync::Arc;

use easel_core::history::HistoryStore;
use easel_core::job::JobState;
use easel_core::providers::JobId;
use easel_core::providers::bfl::flux3::Flux3;
use easel_core::providers::bfl::protocol::extract_result_mime;
use easel_core::text::sanitize_for_display;
use gpui::{
    Bounds, Context, IntoElement, MouseButton, ParentElement, Pixels, Point, Render, RenderImage,
    StatefulInteractiveElement, Styled, Window, canvas, div, img, point, prelude::*, px, rgb,
};

use crate::app_state::AppState;
use crate::image_load::decode_image;
use crate::jobs::{JobRunner, LiveJob, inspection_from, promote};
use crate::theme;
use crate::ui::Button;

const CARD_LIMIT: usize = 2 * 1024;
const DETAIL_LIMIT: usize = 16 * 1024;

struct Row {
    token: Option<u64>,
    id: Option<JobId>,
    state: JobState,
    line: String,
    #[cfg_attr(not(test), allow(dead_code))]
    detail: String,
    cost: Option<f64>,
    bytes: Option<Vec<u8>>,
    thumb: Option<Arc<RenderImage>>,
}

pub struct BottomStrip {
    app_state: gpui::Entity<AppState>,
    runner: Option<gpui::Entity<JobRunner<Flux3>>>,
    history: Arc<HistoryStore>,
    rows: Vec<Row>,
    bounds: Vec<Bounds<Pixels>>,
    selected: Option<usize>,
    pending_discard: bool,
    thumbs: HashMap<String, Arc<RenderImage>>,
}

impl BottomStrip {
    pub fn new(
        app_state: gpui::Entity<AppState>,
        runner: Option<gpui::Entity<JobRunner<Flux3>>>,
        history: Arc<HistoryStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        if let Some(runner) = &runner {
            let watched = runner.clone();
            cx.observe(&watched, |this, _, cx| {
                this.rebuild(cx);
                cx.notify();
            })
            .detach();
        }
        Self {
            app_state,
            runner,
            history,
            rows: Vec::new(),
            bounds: Vec::new(),
            selected: None,
            pending_discard: false,
            thumbs: HashMap::new(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn app_state(&self) -> gpui::Entity<AppState> {
        self.app_state.clone()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.rebuild(cx);
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn lines(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.line.clone()).collect()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn costs(&self) -> Vec<Option<f64>> {
        self.rows.iter().map(|row| row.cost).collect()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn detail(&self) -> String {
        self.selected
            .and_then(|index| self.rows.get(index))
            .map(|row| row.detail.clone())
            .unwrap_or_default()
    }

    pub fn actions(&self) -> Vec<String> {
        if self.pending_discard {
            return vec!["Confirm".into(), "Cancel".into()];
        }
        let Some(row) = self.selected.and_then(|index| self.rows.get(index)) else {
            return Vec::new();
        };
        actions_for(&row.state)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn confirm_visible(&self) -> bool {
        self.pending_discard
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn entry_point(&self, index: usize) -> Point<Pixels> {
        let bounds = self.bounds.get(index).copied().unwrap_or_default();
        point(
            bounds.left() + bounds.size.width / 2.0,
            bounds.top() + bounds.size.height / 2.0,
        )
    }

    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.rows.len() {
            return;
        }
        self.selected = Some(index);
        self.pending_discard = false;
        cx.notify();
    }

    pub fn press(&mut self, action: &str, cx: &mut Context<Self>) {
        match action {
            "Promote" => self.promote_selected(cx),
            "Save As" => self.save_selected(cx),
            "Compare" => self.compare_selected(cx),
            "Discard" => {
                self.pending_discard = true;
                cx.notify();
            }
            "Confirm" => self.confirm_discard(cx),
            "Cancel" => {
                self.pending_discard = false;
                cx.notify();
            }
            "Dismiss" => self.dismiss_selected(cx),
            _ => {}
        }
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let live = self
            .runner
            .as_ref()
            .map(|runner| runner.read(cx).live_jobs())
            .unwrap_or_default();
        let listed = self.history.list().unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let mut rows = Vec::new();
        for job in live {
            if let Some(id) = &job.id {
                seen.insert(id.as_str().to_string());
            }
            rows.push(self.row_from_live(job));
        }
        for summary in listed {
            if seen.contains(summary.id.as_str()) {
                continue;
            }
            rows.push(self.row_from_history(summary.state, summary.cost, summary.id));
        }
        if self.selected.is_some_and(|index| index >= rows.len()) {
            self.selected = None;
            self.pending_discard = false;
        }
        self.bounds.resize(rows.len(), Bounds::default());
        self.rows = rows;
    }

    fn row_from_live(&mut self, job: LiveJob) -> Row {
        let (line, detail) = copy_for(&job.state, &job.status);
        let thumb = job.id.as_ref().and_then(|id| self.thumb_for(id));
        Row {
            token: Some(job.token),
            id: job.id,
            state: job.state,
            line,
            detail,
            cost: job.cost,
            bytes: job.bytes,
            thumb,
        }
    }

    fn row_from_history(&mut self, state: JobState, cost: Option<f64>, id: JobId) -> Row {
        let (line, detail) = copy_for(&state, "");
        let bytes = self.history.result_bytes(&id).ok();
        let thumb = self.thumb_for(&id);
        Row {
            token: None,
            id: Some(id),
            state,
            line,
            detail,
            cost,
            bytes,
            thumb,
        }
    }

    fn thumb_for(&mut self, id: &JobId) -> Option<Arc<RenderImage>> {
        if let Some(image) = self.thumbs.get(id.as_str()) {
            return Some(Arc::clone(image));
        }
        let bytes = self.history.thumb_bytes(id)?;
        let decoded = decode_image(&bytes, "thumb").ok()?;
        let image = Arc::clone(&decoded.render);
        self.thumbs
            .insert(id.as_str().to_string(), Arc::clone(&image));
        Some(image)
    }

    fn promote_selected(&mut self, cx: &mut Context<Self>) {
        let Some(bytes) = self.selected_bytes() else {
            return;
        };
        let state = self.app_state.clone();
        let ok = state.update(cx, |app, cx| promote(app, &bytes, cx)).is_ok();
        if ok {
            cx.notify();
        }
    }

    fn compare_selected(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .selected
            .and_then(|index| self.rows.get(index))
            .and_then(|row| row.id.clone())
        else {
            return;
        };
        let Ok(loaded) = self.history.load_request(&id) else {
            return;
        };
        let inspection = inspection_from(&loaded);
        self.app_state.update(cx, |app, cx| {
            app.inspection = Some(inspection);
            cx.notify();
        });
    }

    fn save_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected.and_then(|index| self.rows.get(index)) else {
            return;
        };
        let Some(bytes) = row.bytes.clone().or_else(|| {
            row.id
                .as_ref()
                .and_then(|id| self.history.result_bytes(id).ok())
        }) else {
            return;
        };
        let ext = extension_for(&bytes);
        let name = match &row.id {
            Some(id) => format!("easel-{}.{ext}", id.as_str()),
            None => format!("easel-result.{ext}"),
        };
        let receiver = cx.prompt_for_new_path(std::path::Path::new("."), Some(&name));
        cx.spawn(async move |_this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            cx.background_executor()
                .spawn(async move {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::write(path, bytes)
                })
                .await
                .ok();
        })
        .detach();
    }

    fn confirm_discard(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.selected else {
            return;
        };
        let Some(row) = self.rows.get(index) else {
            return;
        };
        if let Some(id) = row.id.clone() {
            let _ = self.history.remove(&id);
        }
        if let Some(token) = row.token
            && let Some(runner) = &self.runner
        {
            runner.update(cx, |runner, _| runner.forget_token(token));
        }
        self.pending_discard = false;
        self.selected = None;
        self.rebuild(cx);
        cx.notify();
    }

    fn dismiss_selected(&mut self, cx: &mut Context<Self>) {
        let Some(token) = self
            .selected
            .and_then(|index| self.rows.get(index))
            .and_then(|row| row.token)
        else {
            return;
        };
        if let Some(runner) = &self.runner {
            runner.update(cx, |runner, cx| runner.dismiss_token(token, cx));
        }
        self.rebuild(cx);
        cx.notify();
    }

    fn selected_bytes(&self) -> Option<Vec<u8>> {
        let row = self.selected.and_then(|index| self.rows.get(index))?;
        if let Some(bytes) = &row.bytes {
            return Some(bytes.clone());
        }
        row.id
            .as_ref()
            .and_then(|id| self.history.result_bytes(id).ok())
    }
}

fn actions_for(state: &JobState) -> Vec<&'static str> {
    if state.is_in_flight() {
        vec!["Dismiss"]
    } else if matches!(state, JobState::Done { .. }) {
        vec!["Promote", "Save As", "Compare", "Discard"]
    } else {
        vec!["Discard"]
    }
}

fn copy_for(state: &JobState, status: &str) -> (String, String) {
    match state {
        JobState::Queued => ("Queued".into(), String::new()),
        JobState::Submitting => ("Submitting".into(), String::new()),
        JobState::Polling { progress, .. } => {
            let label = if status.is_empty() { "Polling" } else { status };
            let line = match progress {
                Some(value) => format!(
                    "{label} {}%",
                    (value.clamp(0.0, 1.0) * 100.0).round() as i32
                ),
                None => label.to_string(),
            };
            (line, String::new())
        }
        JobState::Downloading => ("Downloading".into(), String::new()),
        JobState::Done { cost, .. } => {
            let line = match cost {
                Some(cost) => format!("Done {cost:.2}"),
                None => "Done".into(),
            };
            (line, String::new())
        }
        JobState::Failed { error } => {
            let text = error.to_string();
            (
                sanitize_for_display(&text, CARD_LIMIT),
                sanitize_for_display(&text, DETAIL_LIMIT),
            )
        }
        JobState::Expired => ("Expired".into(), String::new()),
        JobState::Dismissed => ("Dismissed".into(), String::new()),
    }
}

fn extension_for(bytes: &[u8]) -> &'static str {
    match extract_result_mime(None, bytes) {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/webp") => "webp",
        _ => "bin",
    }
}

impl Render for BottomStrip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.rebuild(cx);
        let selected = self.selected;
        let pending = self.pending_discard;
        let view = cx.entity();
        let mut entries = Vec::new();
        for (index, row) in self.rows.iter().enumerate() {
            let selected_entry = selected == Some(index);
            let border = if selected_entry {
                theme::BORDER_FOCUSED
            } else {
                theme::BORDER
            };
            let line = row.line.clone();
            let cost = row.cost.map(|cost| format!("{cost:.2}"));
            let thumb = row.thumb.clone();
            let bounds_view = view.clone();
            entries.push(
                div()
                    .id(("history-entry", index))
                    .relative()
                    .flex()
                    .flex_none()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .w(px(160.))
                    .h(px(48.))
                    .px(px(6.))
                    .border_1()
                    .border_color(rgb(border))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            if event.click_count >= 2 {
                                this.select(index, cx);
                                this.press("Promote", cx);
                            } else {
                                this.select(index, cx);
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, _, cx| this.select(index, cx)),
                    )
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                bounds_view.update(cx, |strip, _| {
                                    if index < strip.bounds.len() {
                                        strip.bounds[index] = bounds;
                                    }
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .children(thumb.map(|image| img(image).w(px(40.)).h(px(40.))))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w(px(0.))
                            .child(div().text_xs().child(line))
                            .children(cost.map(|cost| {
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::TEXT_MUTED))
                                    .child(cost)
                            })),
                    ),
            );
        }
        let actions = self.actions();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w_full()
            .bg(rgb(theme::PANEL))
            .border_t_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .child(
                div()
                    .id("bottom-strip")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .w_full()
                    .h(px(theme::STRIP_HEIGHT))
                    .px(px(12.))
                    .overflow_x_scroll()
                    .children(entries),
            )
            .when(selected.is_some(), |strip| {
                strip.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .h(px(theme::PANE_HEADER_HEIGHT))
                        .px(px(12.))
                        .border_t_1()
                        .border_color(rgb(theme::BORDER))
                        .when(pending, |row| {
                            row.child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::TEXT_MUTED))
                                    .child("Discard this result?"),
                            )
                        })
                        .children(actions.into_iter().map(|label| {
                            let name = label.clone();
                            let view = view.clone();
                            Button::ghost(format!("strip-{name}"), label).on_click(
                                move |_, _, cx| {
                                    view.update(cx, |strip, cx| strip.press(&name, cx));
                                },
                            )
                        })),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    include!("bottom_strip_tests.rs");
}
