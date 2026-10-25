use std::collections::HashMap;
use std::io::ErrorKind;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use easel_core::history::{GridMeta, HistoryStore};
use easel_core::job::JobState;
use easel_core::providers::JobId;
use easel_core::providers::bfl::protocol::extract_result_mime;
use easel_core::text::sanitize_for_display;
use gpui::{
    Bounds, Context, IntoElement, MouseButton, ParentElement, Pixels, Point, Render, RenderImage,
    StatefulInteractiveElement, Styled, Window, canvas, div, img, point, prelude::*, px, rgb,
};

use crate::app_state::{AppState, HistoryItem};
use crate::image_load::decode_image;
use crate::jobs::{JobRunner, LiveJob, inspection_from, promote};
use crate::notifications::Notifications;
use crate::theme;
use crate::ui::pane_header;

const CARD_LIMIT: usize = 2 * 1024;
const DETAIL_LIMIT: usize = 16 * 1024;
const OPEN_AFTER: Duration = Duration::from_millis(400);

struct ArmedClick {
    item: HistoryItem,
    generation: u64,
}

struct Preview {
    image: Arc<RenderImage>,
    width: u32,
    height: u32,
}

pub struct HistoryDialog {
    pub image: Option<Arc<RenderImage>>,
    pub pixel_size: Option<(u32, u32)>,
    pub line: String,
    pub detail: String,
    pub cost: Option<f64>,
    pub model: String,
    pub prompt: String,
    pub actions: Vec<String>,
    pub confirming: bool,
}

pub struct GridDialog {
    pub images: Vec<Option<Arc<RenderImage>>>,
    pub picked: [bool; 4],
    pub selected: [bool; 4],
    pub line: String,
    pub cost: Option<f64>,
    pub model: String,
    pub prompt: String,
    pub actions: Vec<String>,
    pub confirming: bool,
}

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
    prompt: String,
    model: String,
    editor: Option<easel_core::history::EditorSnapshot>,
    grid: Option<GridMeta>,
}

pub struct BottomStrip {
    app_state: gpui::Entity<AppState>,
    runner: Option<gpui::Entity<JobRunner>>,
    history: Arc<HistoryStore>,
    rows: Vec<Row>,
    bounds: Vec<Bounds<Pixels>>,
    selected: Option<usize>,
    pending_discard: bool,
    armed: Option<ArmedClick>,
    arm_generation: u64,
    thumbs: HashMap<String, Arc<RenderImage>>,
    previews: HashMap<String, Preview>,
    grid_images: HashMap<String, Vec<Option<Arc<RenderImage>>>>,
    notes: Option<gpui::Entity<Notifications>>,
    reveal: Arc<dyn Fn(&std::path::Path) -> Result<(), String> + Send + Sync>,
    grid_selection: [bool; 4],
    grid_open_for: Option<HistoryItem>,
}

impl BottomStrip {
    pub fn new(
        app_state: gpui::Entity<AppState>,
        runner: Option<gpui::Entity<JobRunner>>,
        history: Arc<HistoryStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        if let Some(runner) = &runner {
            runner.update(cx, |runner, _| runner.watch_app_state(app_state.clone()));
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
            armed: None,
            arm_generation: 0,
            thumbs: HashMap::new(),
            previews: HashMap::new(),
            grid_images: HashMap::new(),
            notes: None,
            reveal: Arc::new(open_result_folder),
            grid_selection: [false; 4],
            grid_open_for: None,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn app_state(&self) -> gpui::Entity<AppState> {
        self.app_state.clone()
    }

    #[cfg(test)]
    pub fn show(&mut self, index: usize, cx: &mut Context<Self>) {
        self.rebuild(cx);
        self.open_row(index, cx);
    }

    #[cfg(test)]
    pub fn history(&self) -> Arc<HistoryStore> {
        Arc::clone(&self.history)
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

    #[cfg_attr(not(test), allow(dead_code))]
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

    #[cfg_attr(not(test), allow(dead_code))]
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
            "Reveal" => self.reveal_selected(cx),
            "Compare" => self.compare_selected(cx),
            "Restore" => self.restore_selected(cx),
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
        let persisted: std::collections::HashSet<String> = listed
            .iter()
            .map(|summary| summary.id.as_str().to_string())
            .collect();
        let mut seen = std::collections::HashSet::new();
        let mut rows = Vec::new();
        for job in live {
            let persisted_done = matches!(job.state, JobState::Done { .. })
                && job
                    .id
                    .as_ref()
                    .is_some_and(|id| persisted.contains(id.as_str()));
            if persisted_done {
                continue;
            }
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
        let grid = job.id.as_ref().and_then(|id| self.history.grid_meta(id));
        Row {
            token: Some(job.token),
            id: job.id.clone(),
            state: job.state,
            line,
            detail,
            cost: job.cost,
            bytes: job.bytes,
            thumb,
            prompt: self.prompt_for(job.prose, job.id.as_ref()),
            model: self.model_for(Some(job.model_id), job.id.as_ref()),
            editor: job.editor,
            grid,
        }
    }

    fn row_from_history(&mut self, state: JobState, cost: Option<f64>, id: JobId) -> Row {
        let (line, detail) = copy_for(&state, "");
        let bytes = self.history.result_bytes(&id).ok();
        let thumb = self.thumb_for(&id);
        let editor = self.history.load_editor(&id).ok().flatten();
        let grid = self.history.grid_meta(&id);
        Row {
            token: None,
            id: Some(id.clone()),
            state,
            line,
            detail,
            cost,
            bytes,
            thumb,
            prompt: self.prompt_for(
                editor.as_ref().map(|editor| editor.prose.clone()),
                Some(&id),
            ),
            model: self.model_for(
                editor.as_ref().map(|editor| editor.model_id.clone()),
                Some(&id),
            ),
            editor,
            grid,
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
        let Some(index) = self.action_index(cx) else {
            return;
        };
        self.promote_index(index, cx);
    }

    fn promote_index(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(bytes) = self.bytes_at(index) else {
            return;
        };
        let state = self.app_state.clone();
        let ok = state.update(cx, |app, cx| promote(app, &bytes, cx)).is_ok();
        if ok {
            self.close_modal(cx);
        }
    }

    fn compare_selected(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .action_index(cx)
            .and_then(|index| self.rows.get(index))
            .and_then(|row| row.id.clone())
        else {
            return;
        };
        let Ok(loaded) = self.history.load_request(&id) else {
            return;
        };
        let mut inspection = inspection_from(&loaded);
        inspection.suggestion =
            easel_core::text::sanitize_for_display(&self.history.suggestion(&id), DETAIL_LIMIT);
        let model_id = loaded.model_id.clone();
        let params = loaded.params.into_iter().collect();
        self.app_state.update(cx, move |app, cx| {
            app.set_model(&model_id, cx);
            app.values = app.reconcile_values(params);
            app.inspection = Some(inspection);
            cx.notify();
        });
        self.close_modal(cx);
    }

    fn restore_selected(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self
            .action_index(cx)
            .and_then(|index| self.rows.get(index))
            .and_then(|row| row.editor.clone())
        else {
            return;
        };
        let known = easel_core::providers::ModelRegistry::specs()
            .iter()
            .any(|spec| spec.id == editor.model_id);
        if !known {
            self.report_status(&format!("Unknown model {}", editor.model_id), cx);
            return;
        }
        self.app_state.update(cx, |app, cx| {
            app.inspection = None;
            app.set_model(&editor.model_id, cx);
            app.values = app.reconcile_values(editor.values.clone());
            app.scene.prose = editor.prose.clone();
            if !editor.aspect_ratio.is_empty() {
                app.set_aspect_ratio(&editor.aspect_ratio, cx);
            }
            cx.notify();
        });
        self.close_modal(cx);
    }

    fn report_status(&self, text: &str, cx: &mut Context<Self>) {
        let Some(notes) = self.notes.clone() else {
            return;
        };
        notes.update(cx, |notes, cx| notes.set_status(text.to_owned(), cx));
    }

    pub fn set_notes(&mut self, notes: gpui::Entity<Notifications>) {
        self.notes = Some(notes);
    }

    #[cfg(test)]
    pub fn set_reveal(
        &mut self,
        reveal: Arc<dyn Fn(&std::path::Path) -> Result<(), String> + Send + Sync>,
    ) {
        self.reveal = reveal;
    }

    fn reveal_selected(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .action_index(cx)
            .and_then(|index| self.rows.get(index))
            .and_then(|row| row.id.clone())
        else {
            return;
        };
        let Some(path) = self.history.result_path(&id) else {
            return;
        };
        let Some(folder) = path.parent() else {
            return;
        };
        if let Err(error) = (self.reveal)(folder) {
            self.report_status(&error, cx);
        }
    }

    fn save_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.action_index(cx).and_then(|index| self.rows.get(index)) else {
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
        let Some(index) = self.action_index(cx) else {
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
        self.close_modal(cx);
    }

    fn dismiss_selected(&mut self, cx: &mut Context<Self>) {
        let Some(token) = self
            .action_index(cx)
            .and_then(|index| self.rows.get(index))
            .and_then(|row| row.token)
        else {
            return;
        };
        if let Some(runner) = &self.runner {
            runner.update(cx, |runner, cx| runner.dismiss_token(token, cx));
        }
        self.rebuild(cx);
        self.sync_modal(cx);
        cx.notify();
    }

    fn bytes_at(&self, index: usize) -> Option<Vec<u8>> {
        let row = self.rows.get(index)?;
        if let Some(bytes) = &row.bytes {
            return Some(bytes.clone());
        }
        row.id
            .as_ref()
            .and_then(|id| self.history.result_bytes(id).ok())
    }

    fn action_index(&self, cx: &Context<Self>) -> Option<usize> {
        if let Some(item) = self.app_state.read(cx).history_item.clone() {
            return self.row_for(&item);
        }
        self.selected
    }

    fn row_for(&self, item: &HistoryItem) -> Option<usize> {
        self.rows.iter().position(|row| {
            (item.token.is_some() && row.token == item.token)
                || (item.id.is_some() && row.id == item.id)
        })
    }

    pub fn dismiss_modal(&mut self, cx: &mut Context<Self>) {
        self.cancel_arm();
        self.close_modal(cx);
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.pending_discard = false;
        self.grid_selection = [false; 4];
        self.grid_open_for = None;
        self.app_state
            .update(cx, |app, cx| app.set_history_item(None, cx));
        cx.notify();
    }

    fn sync_modal(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.app_state.read(cx).history_item.clone() else {
            return;
        };
        let Some(index) = self.row_for(&item) else {
            self.close_modal(cx);
            return;
        };
        let row = &self.rows[index];
        let next = HistoryItem {
            token: row.token.or(item.token),
            id: row.id.clone().or(item.id.clone()),
        };
        if next != item {
            self.app_state
                .update(cx, |app, cx| app.set_history_item(Some(next), cx));
        }
    }

    pub fn dialog(&mut self, cx: &mut Context<Self>) -> Option<HistoryDialog> {
        self.rebuild(cx);
        self.sync_modal(cx);
        let item = self.app_state.read(cx).history_item.clone()?;
        let index = self.row_for(&item)?;
        if self.rows[index].grid.is_some() {
            return None;
        }
        let id = self.rows[index].id.clone();
        let line = self.rows[index].line.clone();
        let detail = self.rows[index].detail.clone();
        let cost = self.rows[index].cost;
        let model = self.rows[index].model.clone();
        let prompt = self.rows[index].prompt.clone();
        let state = self.rows[index].state.clone();
        let has_editor = self.rows[index].editor.is_some();
        let has_file = id
            .as_ref()
            .is_some_and(|id| self.history.result_path(id).is_some());
        let preview = id.as_ref().and_then(|id| self.result_preview(id));
        let show_detail = detail.len() > line.len();
        let actions = self.row_actions(&state, has_editor, has_file, false);
        Some(HistoryDialog {
            image: preview.as_ref().map(|preview| Arc::clone(&preview.image)),
            pixel_size: preview.map(|preview| (preview.width, preview.height)),
            line,
            detail: if show_detail { detail } else { String::new() },
            cost,
            model,
            prompt,
            actions,
            confirming: self.pending_discard,
        })
    }

    pub fn grid_dialog(&mut self, cx: &mut Context<Self>) -> Option<GridDialog> {
        self.app_state.read(cx).history_item.as_ref()?;
        self.rebuild(cx);
        self.sync_modal(cx);
        let item = self.app_state.read(cx).history_item.clone()?;
        let index = self.row_for(&item)?;
        let grid = self.rows[index].grid.clone()?;
        if self.grid_open_for.as_ref() != Some(&item) {
            self.grid_selection = [false; 4];
            self.grid_open_for = Some(item);
        }
        let id = self.rows[index].id.clone()?;
        let line = self.rows[index].line.clone();
        let cost = self.rows[index].cost;
        let model = self.rows[index].model.clone();
        let prompt = self.rows[index].prompt.clone();
        let state = self.rows[index].state.clone();
        let has_editor = self.rows[index].editor.is_some();
        let has_file = self.history.result_path(&id).is_some();
        let picked = grid.picked;
        let images = self.quadrant_images(&id);
        let actions = self.row_actions(&state, has_editor, has_file, true);
        Some(GridDialog {
            images,
            picked,
            selected: self.grid_selection,
            line,
            cost,
            model,
            prompt,
            actions,
            confirming: self.pending_discard,
        })
    }

    fn quadrant_images(&mut self, id: &JobId) -> Vec<Option<Arc<RenderImage>>> {
        if let Some(cached) = self.grid_images.get(id.as_str()) {
            return cached.clone();
        }
        let images: Vec<Option<Arc<RenderImage>>> = self
            .history
            .quadrant_bytes(id, 0..4)
            .into_iter()
            .map(|bytes| {
                bytes
                    .and_then(|bytes| decode_image(&bytes, "quadrant").ok())
                    .map(|decoded| decoded.render)
            })
            .collect();
        if images.iter().all(Option::is_some) {
            self.grid_images
                .insert(id.as_str().to_string(), images.clone());
        }
        images
    }

    fn row_actions(
        &self,
        state: &JobState,
        has_editor: bool,
        has_file: bool,
        hide_promote: bool,
    ) -> Vec<String> {
        if self.pending_discard {
            return vec!["Confirm".into(), "Cancel".into()];
        }
        let mut actions: Vec<String> = actions_for(state)
            .into_iter()
            .filter(|label| !hide_promote || *label != "Promote")
            .map(str::to_string)
            .collect();
        if has_editor {
            actions.insert(0, "Restore".into());
        }
        if has_file {
            let at = actions
                .iter()
                .position(|label| label == "Save As")
                .map(|index| index + 1)
                .unwrap_or(actions.len());
            actions.insert(at, "Reveal".into());
        }
        actions
    }

    pub fn toggle_grid_quadrant(&mut self, quadrant: usize, cx: &mut Context<Self>) {
        let Some(item) = self.app_state.read(cx).history_item.clone() else {
            return;
        };
        let Some(index) = self.row_for(&item) else {
            return;
        };
        let Some(grid) = self.rows.get(index).and_then(|row| row.grid.as_ref()) else {
            return;
        };
        if quadrant >= 4 || grid.picked[quadrant] {
            return;
        }
        self.grid_selection[quadrant] = !self.grid_selection[quadrant];
        cx.notify();
    }

    pub fn confirm_grid_pick(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.app_state.read(cx).history_item.clone() else {
            return;
        };
        let Some(index) = self.row_for(&item) else {
            return;
        };
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let Some(grid) = row.grid.clone() else {
            return;
        };
        let Some(id) = row.id.clone() else {
            return;
        };
        let selection: Vec<usize> = (0..4)
            .filter(|quadrant| self.grid_selection[*quadrant] && !grid.picked[*quadrant])
            .collect();
        if selection.is_empty() {
            return;
        }
        let created_at = crate::jobs::utc_now();
        let _ = self.history.pick_quadrants(&id, &selection, &created_at);
        self.rebuild(cx);
        self.close_modal(cx);
        cx.notify();
    }

    fn prompt_for(&self, prose: Option<String>, id: Option<&JobId>) -> String {
        if let Some(prose) = prose {
            return prose;
        }
        let Some(id) = id else {
            return String::new();
        };
        if let Some(editor) = self.history.load_editor(id).ok().flatten() {
            return editor.prose;
        }
        self.history
            .load_request(id)
            .map(|loaded| loaded.compiled_prompt)
            .unwrap_or_default()
    }

    fn model_for(&self, model: Option<String>, id: Option<&JobId>) -> String {
        if let Some(model) = model.filter(|model| !model.is_empty()) {
            return model;
        }
        let Some(id) = id else {
            return String::new();
        };
        if let Some(editor) = self.history.load_editor(id).ok().flatten() {
            return editor.model_id;
        }
        self.history
            .load_request(id)
            .map(|loaded| loaded.model_id)
            .unwrap_or_default()
    }

    fn result_preview(&mut self, id: &JobId) -> Option<Preview> {
        if let Some(preview) = self.previews.get(id.as_str()) {
            return Some(Preview {
                image: Arc::clone(&preview.image),
                width: preview.width,
                height: preview.height,
            });
        }
        let bytes = self.history.result_bytes(id).ok()?;
        let decoded = decode_image(&bytes, "result").ok()?;
        let preview = Preview {
            image: Arc::clone(&decoded.render),
            width: decoded.size.w,
            height: decoded.size.h,
        };
        self.previews.insert(
            id.as_str().to_string(),
            Preview {
                image: Arc::clone(&preview.image),
                width: preview.width,
                height: preview.height,
            },
        );
        Some(preview)
    }

    fn open_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let item = HistoryItem {
            token: row.token,
            id: row.id.clone(),
        };
        self.pending_discard = false;
        self.armed = None;
        self.selected = Some(index);
        self.app_state
            .update(cx, |app, cx| app.set_history_item(Some(item), cx));
        cx.notify();
    }

    fn arm_open(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        self.arm_generation += 1;
        let generation = self.arm_generation;
        let item = HistoryItem {
            token: row.token,
            id: row.id.clone(),
        };
        self.armed = Some(ArmedClick {
            item: item.clone(),
            generation,
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(OPEN_AFTER).await;
            this.update(cx, |this, cx| {
                let still = this
                    .armed
                    .as_ref()
                    .is_some_and(|armed| armed.generation == generation && armed.item == item);
                if !still {
                    return;
                }
                this.armed = None;
                this.rebuild(cx);
                if let Some(index) = this.row_for(&item) {
                    this.open_row(index, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn cancel_arm(&mut self) {
        self.arm_generation += 1;
        self.armed = None;
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

fn open_result_folder(folder: &std::path::Path) -> Result<(), String> {
    let output = Command::new("xdg-open").arg(folder).output();
    match output {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err("Could not open the folder".into())
        }
        Err(error) => Err(error.to_string()),
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stderr);
            let text = text.trim();
            if text.is_empty() {
                Err("Could not open the folder".into())
            } else {
                Err(text.to_string())
            }
        }
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
                    .w_full()
                    .h(px(48.))
                    .px(px(6.))
                    .border_1()
                    .border_color(rgb(border))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            if this.rows.get(index).is_some_and(|row| row.grid.is_some()) {
                                this.cancel_arm();
                                this.open_row(index, cx);
                                return;
                            }
                            let same = this
                                .armed
                                .as_ref()
                                .is_some_and(|armed| this.row_for(&armed.item) == Some(index));
                            if event.click_count >= 2 || same {
                                this.cancel_arm();
                                this.promote_index(index, cx);
                            } else {
                                this.arm_open(index, cx);
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, _, cx| {
                            this.cancel_arm();
                            this.open_row(index, cx);
                        }),
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
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(theme::HISTORY_WIDTH))
            .h_full()
            .min_h(px(0.))
            .bg(rgb(theme::PANEL))
            .border_l_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .child(pane_header("History"))
            .child(
                div()
                    .id("bottom-strip")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.))
                    .gap(px(8.))
                    .w_full()
                    .p(px(8.))
                    .overflow_y_scroll()
                    .children(entries),
            )
    }
}

#[cfg(test)]
mod tests {
    include!("bottom_strip_tests.rs");
}
