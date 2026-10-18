#![cfg_attr(not(test), allow(dead_code))]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use easel_core::error::ProviderError;
use easel_core::history::{Completion, HistoryStore};
use easel_core::http::HttpResponse;
use easel_core::job::{JobEvent, JobRequest, JobState, advance};
use easel_core::model_spec::ProviderId;
use easel_core::providers::{ImageInput, PollOutcome, Provider, ReferenceStep, Submitted};
use gpui::Context;

use crate::image_load::decode_image;
use crate::net::{HttpTransport, KeyResolver, TransportError};

const MAX_ACTIVE: usize = 4;

struct Slot {
    token: u64,
    state: JobState,
    request: JobRequest,
    editor: Option<easel_core::history::EditorSnapshot>,
    local_id: Option<easel_core::providers::JobId>,
    submitted: Option<Submitted>,
    elapsed: Duration,
    bytes: Option<Vec<u8>>,
    dead: bool,
    status: String,
    cost: Option<f64>,
    last_outcome: Option<PollOutcome>,
}

pub struct LiveJob {
    pub token: u64,
    pub id: Option<easel_core::providers::JobId>,
    pub state: JobState,
    pub cost: Option<f64>,
    pub status: String,
    pub bytes: Option<Vec<u8>>,
    /// Typed prompt from the Generate snapshot. Absent on rows submitted before it existed.
    pub prose: Option<String>,
    pub model_id: String,
    pub editor: Option<easel_core::history::EditorSnapshot>,
}

pub struct JobRunner {
    resolve: Arc<dyn Fn(&str) -> Option<Box<dyn Provider>>>,
    transport: Arc<dyn HttpTransport>,
    keys: Arc<dyn KeyResolver>,
    history: Arc<HistoryStore>,
    slots: Vec<Slot>,
    providers: HashMap<u64, Box<dyn Provider>>,
    uploads: HashMap<String, ImageInput>,
    next_token: u64,
    sunset_watch: Option<(
        gpui::Entity<crate::app_state::AppState>,
        gpui::Entity<crate::notifications::Notifications>,
    )>,
}

impl JobRunner {
    pub fn new<R>(
        resolve: R,
        transport: Arc<dyn HttpTransport>,
        keys: Arc<dyn KeyResolver>,
        history: Arc<HistoryStore>,
    ) -> Self
    where
        R: Fn(&str) -> Option<Box<dyn Provider>> + 'static,
    {
        Self {
            resolve: Arc::new(resolve),
            transport,
            keys,
            history,
            slots: Vec::new(),
            providers: HashMap::new(),
            uploads: HashMap::new(),
            next_token: 1,
            sunset_watch: None,
        }
    }

    pub fn submit(&mut self, request: JobRequest, cx: &mut Context<Self>) {
        self.submit_recorded(request, None, cx);
    }

    pub fn submit_recorded(
        &mut self,
        request: JobRequest,
        editor: Option<easel_core::history::EditorSnapshot>,
        cx: &mut Context<Self>,
    ) {
        let token = self.next_token;
        self.next_token += 1;
        let model_id = request.model_id.clone();
        self.slots.push(Slot {
            token,
            state: JobState::Queued,
            request,
            editor,
            local_id: None,
            submitted: None,
            elapsed: Duration::ZERO,
            bytes: None,
            dead: false,
            status: "Queued".into(),
            cost: None,
            last_outcome: None,
        });
        match (self.resolve)(&model_id) {
            Some(provider) => {
                self.providers.insert(token, provider);
                self.pump(cx);
            }
            None => {
                self.fail(
                    token,
                    ProviderError::Protocol(format!("unknown model {model_id}")),
                    cx,
                );
            }
        }
    }

    #[cfg(test)]
    pub fn first_request(&self) -> Option<JobRequest> {
        self.slots.first().map(|slot| slot.request.clone())
    }

    #[cfg(test)]
    pub fn first_editor(&self) -> Option<easel_core::history::EditorSnapshot> {
        self.slots.first().and_then(|slot| slot.editor.clone())
    }

    pub fn watch_krea_sunset(
        &mut self,
        app: gpui::Entity<crate::app_state::AppState>,
        notes: gpui::Entity<crate::notifications::Notifications>,
    ) {
        self.sunset_watch = Some((app, notes));
    }

    pub fn dismiss(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.slots.get(index).is_none() {
            return;
        }
        if matches!(self.slots[index].state, JobState::Queued) {
            let token = self.slots[index].token;
            self.providers.remove(&token);
            self.slots.remove(index);
            return;
        }
        let token = self.slots[index].token;
        let submitted = self.slots[index].submitted.clone();
        let last = self.slots[index].last_outcome.clone();
        let cancel = submitted.and_then(|submitted| {
            self.providers
                .get(&token)
                .and_then(|provider| provider.cancel_request(&submitted, last.as_ref()))
        });
        let slot = &mut self.slots[index];
        slot.dead = true;
        let state = std::mem::replace(&mut slot.state, JobState::Queued);
        slot.state = advance(state, JobEvent::Dismiss);
        if let Some(id) = slot
            .submitted
            .as_ref()
            .map(|submitted| submitted.job_id.clone())
        {
            let _ = self.history.set_state(&id, JobState::Dismissed);
        }
        cx.notify();
        if let Some(request) = cancel {
            let task = self.transport.request(request, cx);
            cx.spawn(async move |_, _| {
                let _ = task.await;
            })
            .detach();
        }
        self.pump(cx);
    }

    pub fn states(&self) -> Vec<JobState> {
        self.slots.iter().map(|slot| slot.state.clone()).collect()
    }

    pub fn live_jobs(&self) -> Vec<LiveJob> {
        self.slots
            .iter()
            .rev()
            .filter(|slot| !matches!(slot.state, JobState::Dismissed))
            .map(|slot| LiveJob {
                token: slot.token,
                id: slot
                    .submitted
                    .as_ref()
                    .map(|submitted| submitted.job_id.clone())
                    .or_else(|| slot.local_id.clone()),
                state: slot.state.clone(),
                cost: slot.cost,
                status: slot.status.clone(),
                bytes: slot.bytes.clone(),
                prose: slot.editor.as_ref().map(|editor| editor.prose.clone()),
                model_id: slot
                    .editor
                    .as_ref()
                    .map(|editor| editor.model_id.clone())
                    .unwrap_or_else(|| slot.request.model_id.clone()),
                editor: slot.editor.clone(),
            })
            .collect()
    }

    pub fn dismiss_token(&mut self, token: u64, cx: &mut Context<Self>) {
        let Some(index) = self.slots.iter().position(|slot| slot.token == token) else {
            return;
        };
        self.dismiss(index, cx);
    }

    pub fn forget_token(&mut self, token: u64) {
        if let Some(slot) = self.slots.iter().find(|slot| slot.token == token)
            && slot.state.is_in_flight()
        {
            return;
        }
        self.slots.retain(|slot| slot.token != token);
    }

    pub fn result_bytes(&self, index: usize) -> Option<Vec<u8>> {
        self.slots.get(index).and_then(|slot| slot.bytes.clone())
    }

    pub fn resume(&mut self, cx: &mut Context<Self>) {
        let Ok(entries) = self.history.in_flight() else {
            return;
        };
        for entry in entries {
            let Some(poll_url) = entry.poll_url.clone() else {
                continue;
            };
            if entry.provider != "bfl" {
                let error = ProviderError::Protocol("cannot resume this provider".into());
                let _ = self.history.fail(&entry.id, &error);
                continue;
            }
            if !ProviderId::Bfl.allows_url(&poll_url) {
                let error = ProviderError::Protocol("unexpected result host".into());
                let _ = self.history.fail(&entry.id, &error);
                continue;
            }
            let Some(provider) = (self.resolve)("flux-3-image") else {
                let error = ProviderError::Protocol("unknown model flux-3-image".into());
                let _ = self.history.fail(&entry.id, &error);
                continue;
            };
            let Ok(url) = poll_url.parse() else {
                continue;
            };
            let token = self.next_token;
            self.next_token += 1;
            self.providers.insert(token, provider);
            self.slots.push(Slot {
                token,
                state: JobState::Polling {
                    attempt: 0,
                    progress: None,
                },
                request: empty_request(),
                editor: None,
                local_id: None,
                submitted: Some(Submitted {
                    job_id: entry.id,
                    poll_url: url,
                    quote: None,
                }),
                elapsed: Duration::ZERO,
                bytes: None,
                dead: false,
                status: "Polling".into(),
                cost: None,
                last_outcome: None,
            });
            self.schedule_poll(token, cx);
        }
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        while self.active() < MAX_ACTIVE {
            let Some(token) = self
                .slots
                .iter()
                .find(|slot| matches!(slot.state, JobState::Queued) && !slot.dead)
                .map(|slot| slot.token)
            else {
                break;
            };
            if let Some(slot) = self.slot_mut(token) {
                let state = std::mem::replace(&mut slot.state, JobState::Queued);
                slot.state = advance(state, JobEvent::SlotFreed);
            }
            self.resolve(token, Vec::new(), cx);
        }
    }

    fn active(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| {
                !slot.dead && slot.state.is_in_flight() && !matches!(slot.state, JobState::Queued)
            })
            .count()
    }

    fn resolve(&mut self, token: u64, mut inputs: Vec<ImageInput>, cx: &mut Context<Self>) {
        let Some(provider_id) = self
            .providers
            .get(&token)
            .map(|provider| provider.spec().provider)
        else {
            self.fail(token, ProviderError::Protocol("unknown model".into()), cx);
            return;
        };
        if self.keys.key_for(provider_id).is_none() {
            self.fail(token, ProviderError::Auth, cx);
            return;
        }
        let images = self
            .slot_mut(token)
            .map(|slot| slot.request.images.clone())
            .unwrap_or_default();
        let mut steps = self
            .providers
            .get(&token)
            .map(|provider| provider.reference_steps(&images))
            .unwrap_or_default();
        self.resolve_steps(token, &mut steps, &mut inputs, cx);
    }

    fn resolve_steps(
        &mut self,
        token: u64,
        steps: &mut Vec<ReferenceStep>,
        inputs: &mut Vec<ImageInput>,
        cx: &mut Context<Self>,
    ) {
        while let Some(step) = steps.first().cloned() {
            steps.remove(0);
            match step {
                ReferenceStep::Inline(input) => inputs.push(input),
                ReferenceStep::Upload {
                    request,
                    content_hash,
                } => {
                    let cache_key = self.cache_key(token, &content_hash);
                    if let Some(cached) = self.uploads.get(&cache_key).cloned() {
                        inputs.push(cached);
                        continue;
                    }
                    let task = self.transport.request(request, cx);
                    let rest = steps.clone();
                    let inputs = inputs.clone();
                    cx.spawn(async move |this, cx| {
                        let response = task.await;
                        this.update(cx, |runner, cx| {
                            runner.finish_upload(token, cache_key, response, rest, inputs, cx);
                        })
                        .ok();
                    })
                    .detach();
                    return;
                }
            }
        }
        self.send_submit(token, inputs.clone(), cx);
    }

    fn finish_upload(
        &mut self,
        token: u64,
        content_hash: String,
        response: Result<HttpResponse, TransportError>,
        mut steps: Vec<ReferenceStep>,
        mut inputs: Vec<ImageInput>,
        cx: &mut Context<Self>,
    ) {
        if self.dead(token) {
            return;
        }
        let Ok(response) = response else {
            self.fail(token, ProviderError::Network("upload failed".into()), cx);
            return;
        };
        let parsed = self
            .providers
            .get(&token)
            .map(|provider| provider.parse_upload(&response));
        match parsed {
            Some(Ok(input)) => {
                self.uploads.insert(content_hash, input.clone());
                inputs.push(input);
                self.resolve_steps(token, &mut steps, &mut inputs, cx);
            }
            Some(Err(error)) => self.fail(token, error, cx),
            None => self.fail(token, ProviderError::Protocol("unknown model".into()), cx),
        }
    }

    fn send_submit(&mut self, token: u64, inputs: Vec<ImageInput>, cx: &mut Context<Self>) {
        let Some(request) = self.slot_mut(token).map(|slot| slot.request.clone()) else {
            return;
        };
        let Some(http) = self
            .providers
            .get(&token)
            .map(|provider| provider.build_submit(&request, &inputs))
        else {
            self.fail(token, ProviderError::Protocol("unknown model".into()), cx);
            return;
        };
        let task = self.transport.request(http, cx);
        cx.spawn(async move |this, cx| {
            let response = task.await;
            this.update(cx, |runner, cx| runner.finish_submit(token, response, cx))
                .ok();
        })
        .detach();
    }

    fn finish_submit(
        &mut self,
        token: u64,
        response: Result<HttpResponse, TransportError>,
        cx: &mut Context<Self>,
    ) {
        if self.dead(token) {
            return;
        }
        let response = match response {
            Ok(response) => response,
            Err(TransportError::NoCredential(_)) => {
                self.fail(token, ProviderError::Auth, cx);
                return;
            }
            Err(error) => {
                self.fail(
                    token,
                    ProviderError::Network(strip_url(&error.to_string())),
                    cx,
                );
                return;
            }
        };
        self.note_headers(token, &response, cx);
        let inline = self
            .providers
            .get(&token)
            .and_then(|provider| provider.inline_image(&response));
        if let Some(image) = inline {
            self.finish_inline(token, image, cx);
            return;
        }
        let parsed = self
            .providers
            .get(&token)
            .map(|provider| provider.parse_submit(&response));
        match parsed {
            Some(Ok(submitted)) => {
                let recorded = self
                    .slot_mut(token)
                    .map(|slot| (slot.request.clone(), slot.editor.clone()));
                if let Some((request, editor)) = recorded {
                    let _ = self.history.begin(
                        &submitted.job_id,
                        &request,
                        &submitted,
                        editor.as_ref(),
                    );
                }
                if let Some(slot) = self.slot_mut(token) {
                    let state = std::mem::replace(&mut slot.state, JobState::Queued);
                    slot.cost = submitted.quote.as_ref().map(|quote| quote.cost);
                    slot.status = "Submitted".into();
                    slot.state = advance(state, JobEvent::Submitted(submitted.clone()));
                    slot.submitted = Some(submitted);
                }
                cx.notify();
                self.schedule_poll(token, cx);
            }
            Some(Err(error)) => {
                let beta = self
                    .providers
                    .get(&token)
                    .is_some_and(|provider| provider.spec().beta);
                if matches!(
                    error,
                    ProviderError::Blocked
                        | ProviderError::Auth
                        | ProviderError::ModelNotFound
                        | ProviderError::NoImage
                ) && !beta
                {
                    self.finish_unpolled(token, error, cx);
                } else if beta && matches!(error, ProviderError::Auth) {
                    self.probe_beta(token, cx);
                } else {
                    self.fail(token, error, cx);
                }
            }
            None => self.fail(token, ProviderError::Protocol("unknown model".into()), cx),
        }
    }

    fn finish_inline(
        &mut self,
        token: u64,
        image: easel_core::providers::InlineImage,
        cx: &mut Context<Self>,
    ) {
        if self.dead(token) {
            return;
        }
        let Some((request, editor)) = self
            .slot_mut(token)
            .map(|slot| (slot.request.clone(), slot.editor.clone()))
        else {
            return;
        };
        let id = mint_job_id();
        let file_name = match image.mime.as_str() {
            "image/jpeg" => "result.jpg",
            "image/webp" => "result.webp",
            _ => "result.png",
        };
        let thumb = easel_core::image_prep::thumbnail(&image.bytes, 256).unwrap_or_default();
        if let Err(error) = self.history.complete_inline(
            &id,
            &request,
            &image.interaction_id,
            &image.suggestion,
            &image.bytes,
            &image.mime,
            &thumb,
            editor.as_ref(),
        ) {
            self.fail(token, ProviderError::Protocol(error.to_string()), cx);
            return;
        }
        if let Some(slot) = self.slot_mut(token) {
            slot.local_id = Some(id);
            slot.bytes = Some(image.bytes);
            let state = std::mem::replace(&mut slot.state, JobState::Queued);
            slot.state = advance(
                state,
                JobEvent::InlineReady {
                    result_path: format!("history/{file_name}"),
                },
            );
            slot.status = "Done".into();
        }
        cx.notify();
        self.pump(cx);
    }

    fn finish_unpolled(&mut self, token: u64, error: ProviderError, cx: &mut Context<Self>) {
        if self.dead(token) {
            return;
        }
        let Some((request, editor)) = self
            .slot_mut(token)
            .map(|slot| (slot.request.clone(), slot.editor.clone()))
        else {
            return;
        };
        let id = mint_job_id();
        if self
            .history
            .fail_unpolled(&id, &request, &error, editor.as_ref())
            .is_ok()
            && let Some(slot) = self.slot_mut(token)
        {
            slot.local_id = Some(id);
        }
        self.fail(token, error, cx);
    }

    fn probe_beta(&mut self, token: u64, cx: &mut Context<Self>) {
        let Some(provider) = self.providers.get(&token) else {
            return;
        };
        let provider_id = provider.spec().provider;
        let message = format!(
            "{} is in beta; your workspace needs access",
            provider.spec().display_name
        );
        let request = crate::settings::key_probe(provider_id);
        let task = self.transport.request(request, cx);
        cx.spawn(async move |this, cx| {
            let response = task.await;
            this.update(cx, |runner, cx| {
                if runner.dead(token) {
                    return;
                }
                let rejected = matches!(
                    &response,
                    Ok(response) if response.status == 401 || response.status == 403
                );
                if rejected {
                    runner.fail(token, ProviderError::Auth, cx);
                } else {
                    runner.fail(token, ProviderError::Protocol(message), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn schedule_poll(&mut self, token: u64, cx: &mut Context<Self>) {
        let Some((attempt, elapsed)) =
            self.slots
                .iter()
                .find(|slot| slot.token == token)
                .and_then(|slot| match slot.state {
                    JobState::Polling { attempt, .. } => Some((attempt, slot.elapsed)),
                    _ => None,
                })
        else {
            return;
        };
        let Some(policy) = self
            .providers
            .get(&token)
            .map(|provider| provider.poll_policy())
        else {
            return;
        };
        let remaining = policy.deadline.saturating_sub(elapsed);
        if remaining.is_zero() {
            self.deadline(token, cx);
            return;
        }
        let wait = policy.next_delay(attempt).min(remaining);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |runner, cx| runner.on_poll_due(token, wait, cx))
                .ok();
        })
        .detach();
    }

    fn on_poll_due(&mut self, token: u64, wait: Duration, cx: &mut Context<Self>) {
        let dead = self
            .slots
            .iter()
            .find(|slot| slot.token == token)
            .is_none_or(|slot| slot.dead);
        if dead {
            return;
        }
        if let Some(slot) = self.slot_mut(token) {
            slot.elapsed += wait;
        }
        let elapsed = self
            .slots
            .iter()
            .find(|slot| slot.token == token)
            .map(|slot| slot.elapsed)
            .unwrap_or_default();
        let Some(policy) = self
            .providers
            .get(&token)
            .map(|provider| provider.poll_policy())
        else {
            return;
        };
        if policy.expired(elapsed) {
            self.deadline(token, cx);
            return;
        }
        let Some(submitted) = self
            .slots
            .iter()
            .find(|slot| slot.token == token)
            .and_then(|slot| slot.submitted.clone())
        else {
            return;
        };
        let Some(http) = self
            .providers
            .get(&token)
            .map(|provider| provider.poll_request(&submitted))
        else {
            return;
        };
        let task = self.transport.request(http, cx);
        cx.spawn(async move |this, cx| {
            let response = task.await;
            this.update(cx, |runner, cx| runner.finish_poll(token, response, cx))
                .ok();
        })
        .detach();
    }

    fn finish_poll(
        &mut self,
        token: u64,
        response: Result<HttpResponse, TransportError>,
        cx: &mut Context<Self>,
    ) {
        if self.dead(token) {
            return;
        }
        if let Ok(response) = &response {
            self.note_headers(token, response, cx);
        }
        let outcome = match &response {
            Ok(response) => self
                .providers
                .get(&token)
                .map(|provider| provider.parse_poll(response))
                .unwrap_or_else(|| {
                    PollOutcome::Failed(ProviderError::Protocol("unknown model".into()))
                }),
            Err(TransportError::Network { .. } | TransportError::Timeout { .. }) => {
                PollOutcome::Failed(ProviderError::Network("poll failed".into()))
            }
            Err(error) => {
                PollOutcome::Failed(ProviderError::Network(strip_url(&error.to_string())))
            }
        };
        let downloading = matches!(outcome, PollOutcome::Ready { .. });
        if let Some(slot) = self.slot_mut(token) {
            slot.last_outcome = Some(outcome.clone());
        }
        if let PollOutcome::InFlight { status, .. } = &outcome
            && let Some(slot) = self.slot_mut(token)
        {
            slot.status = status.clone();
        }
        if let Some(slot) = self.slot_mut(token) {
            let state = std::mem::replace(&mut slot.state, JobState::Queued);
            slot.state = advance(state, JobEvent::Poll(outcome.clone()));
        }
        match self.slot_mut(token).map(|slot| slot.state.clone()) {
            Some(JobState::Polling { .. }) => {
                cx.notify();
                self.schedule_poll(token, cx);
            }
            Some(JobState::Downloading) if downloading => self.start_download(token, outcome, cx),
            Some(JobState::Failed { error }) => self.persist_failure(token, error, cx),
            Some(state) if state.is_terminal() => self.pump(cx),
            _ => {}
        }
    }

    fn start_download(&mut self, token: u64, outcome: PollOutcome, cx: &mut Context<Self>) {
        let PollOutcome::Ready {
            url,
            settled_cost,
            prompt,
            duration,
        } = outcome
        else {
            return;
        };
        self.arm_deadline(token, cx);
        let Some(http) = self
            .providers
            .get(&token)
            .map(|provider| provider.download_request(&url))
        else {
            return;
        };
        let task = self.transport.request(http, cx);
        cx.spawn(async move |this, cx| {
            let response = task.await;
            this.update(cx, |runner, cx| {
                runner.finish_download(token, response, settled_cost, prompt, duration, cx);
            })
            .ok();
        })
        .detach();
    }

    fn arm_deadline(&mut self, token: u64, cx: &mut Context<Self>) {
        let Some(elapsed) = self
            .slots
            .iter()
            .find(|slot| slot.token == token)
            .map(|slot| slot.elapsed)
        else {
            return;
        };
        let Some(deadline) = self
            .providers
            .get(&token)
            .map(|provider| provider.poll_policy().deadline)
        else {
            return;
        };
        let remaining = deadline.saturating_sub(elapsed);
        if remaining.is_zero() {
            self.deadline(token, cx);
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(remaining).await;
            this.update(cx, |runner, cx| {
                if matches!(
                    runner.slot_mut(token).map(|slot| slot.state.clone()),
                    Some(JobState::Downloading)
                ) {
                    runner.deadline(token, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn finish_download(
        &mut self,
        token: u64,
        response: Result<HttpResponse, TransportError>,
        cost: Option<f64>,
        prompt: Option<String>,
        duration: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        if self.dead(token) || !self.downloading(token) {
            return;
        }
        match response {
            Ok(response) if matches!(response.status, 403 | 404 | 410) => {
                self.expire(token, cx);
            }
            Err(TransportError::TooLarge { .. }) => {
                self.fail(
                    token,
                    ProviderError::Protocol("result exceeded the size limit".into()),
                    cx,
                );
            }
            Err(_) => self.expire(token, cx),
            Ok(response) => {
                if decode_image(&response.body, "result").is_err() {
                    self.fail(
                        token,
                        ProviderError::Protocol("unsupported or oversized result".into()),
                        cx,
                    );
                    return;
                }
                let thumb =
                    easel_core::image_prep::thumbnail(&response.body, 256).unwrap_or_default();
                if let Some(slot) = self.slot_mut(token) {
                    slot.bytes = Some(response.body.clone());
                    if let Some(cost) = cost {
                        slot.cost = Some(cost);
                    }
                }
                let id = self.server_id(token);
                let stored = id.as_ref().and_then(|id| {
                    self.history
                        .complete(
                            id,
                            &response.body,
                            "image/png",
                            &thumb,
                            &Completion {
                                cost,
                                prompt,
                                duration,
                            },
                        )
                        .err()
                });
                if let Some(error) = stored {
                    self.fail(token, ProviderError::Protocol(error.to_string()), cx);
                    return;
                }
                if let Some(slot) = self.slot_mut(token) {
                    let path = id
                        .map(|id| format!("history/{}/result.png", id.as_str()))
                        .unwrap_or_else(|| "result.png".into());
                    let state = std::mem::replace(&mut slot.state, JobState::Queued);
                    slot.state = advance(
                        state,
                        JobEvent::Downloaded {
                            result_path: path,
                            cost,
                        },
                    );
                }
                cx.notify();
                self.pump(cx);
            }
        }
    }

    fn deadline(&mut self, token: u64, cx: &mut Context<Self>) {
        if self.dead(token) {
            return;
        }
        let Some(slot) = self.slot_mut(token) else {
            return;
        };
        if !slot.state.is_in_flight() {
            return;
        }
        slot.dead = true;
        let state = std::mem::replace(&mut slot.state, JobState::Queued);
        let failed = advance(state, JobEvent::Deadline);
        slot.state = failed.clone();
        if let JobState::Failed { error } = failed
            && let Some(id) = slot
                .submitted
                .as_ref()
                .map(|submitted| submitted.job_id.clone())
        {
            let _ = self.history.fail(&id, &error);
        }
        cx.notify();
        self.pump(cx);
    }

    fn expire(&mut self, token: u64, cx: &mut Context<Self>) {
        if let Some(slot) = self.slot_mut(token) {
            let state = std::mem::replace(&mut slot.state, JobState::Queued);
            slot.state = advance(state, JobEvent::DownloadExpired);
            if let Some(id) = slot
                .submitted
                .as_ref()
                .map(|submitted| submitted.job_id.clone())
            {
                let _ = self.history.set_state(&id, JobState::Expired);
            }
        }
        cx.notify();
        self.pump(cx);
    }

    fn note_headers(&mut self, token: u64, response: &HttpResponse, cx: &mut Context<Self>) {
        let is_krea = self
            .providers
            .get(&token)
            .is_some_and(|provider| provider.spec().provider == ProviderId::Krea);
        if !is_krea {
            return;
        }
        let Some(sunset) = easel_core::providers::krea::deprecation_headers(response) else {
            return;
        };
        let Some((app, notes)) = self.sunset_watch.clone() else {
            return;
        };
        app.update(cx, |app, cx| app.note_krea_sunset(sunset, &notes, cx));
    }

    fn fail(&mut self, token: u64, error: ProviderError, cx: &mut Context<Self>) {
        if let Some(slot) = self.slot_mut(token) {
            slot.dead = true;
            slot.state = JobState::Failed {
                error: error.clone(),
            };
            if let Some(id) = slot
                .submitted
                .as_ref()
                .map(|submitted| submitted.job_id.clone())
            {
                let _ = self.history.fail(&id, &error);
            }
        }
        cx.notify();
        self.pump(cx);
    }

    fn persist_failure(&mut self, token: u64, error: ProviderError, cx: &mut Context<Self>) {
        if let Some(id) = self.server_id(token) {
            let _ = self.history.fail(&id, &error);
        }
        if let Some(slot) = self.slot_mut(token) {
            slot.dead = true;
        }
        cx.notify();
        self.pump(cx);
    }

    fn downloading(&self, token: u64) -> bool {
        self.slots.iter().any(|slot| {
            slot.token == token && matches!(slot.state, JobState::Downloading) && !slot.dead
        })
    }

    fn dead(&self, token: u64) -> bool {
        self.slots
            .iter()
            .find(|slot| slot.token == token)
            .is_none_or(|slot| slot.dead)
    }

    fn server_id(&self, token: u64) -> Option<easel_core::providers::JobId> {
        self.slots
            .iter()
            .find(|slot| slot.token == token)
            .and_then(|slot| {
                slot.submitted
                    .as_ref()
                    .map(|submitted| submitted.job_id.clone())
            })
    }

    fn slot_mut(&mut self, token: u64) -> Option<&mut Slot> {
        self.slots.iter_mut().find(|slot| slot.token == token)
    }

    fn cache_key(&self, token: u64, content_hash: &str) -> String {
        let label = self
            .providers
            .get(&token)
            .map(|provider| match provider.spec().provider {
                ProviderId::Bfl => "bfl",
                ProviderId::Krea => "krea",
                ProviderId::Google => "google",
                ProviderId::OpenAi => "openai",
                ProviderId::Xai => "xai",
            })
            .unwrap_or("unknown");
        format!("{label}:{content_hash}")
    }
}

fn mint_job_id() -> easel_core::providers::JobId {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let tick = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    easel_core::providers::JobId::parse(&format!("g{nanos}{tick}")).expect("local job id")
}

fn empty_request() -> JobRequest {
    JobRequest {
        model_id: "flux-3-image".into(),
        compiled_prompt: String::new(),
        params: std::collections::BTreeMap::new(),
        images: Vec::new(),
        box_snapshot: Vec::new(),
        created_at: "2026-10-15T00:00:00Z".into(),
    }
}

fn strip_url(text: &str) -> String {
    text.split_whitespace()
        .filter(|part| !part.contains("://") && !part.contains('?'))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use easel_core::error::ProviderError;
    use easel_core::history::HistoryStore;
    use easel_core::http::{Body, HttpRequest, HttpResponse, Method};
    use easel_core::job::{EncodedImage, JobRequest, JobState};
    use easel_core::model_spec::ProviderId;
    use easel_core::providers::bfl::flux3::Flux3;
    use easel_core::providers::{ImageInput, Provider, ReferenceStep};
    use gpui::{TestAppContext, prelude::*};

    use crate::net::{FakeTransport, HttpTransport, KeyResolver, MapKeys, TransportError};

    use super::JobRunner;

    fn http(status: u16, body: impl AsRef<[u8]>) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_ref().to_vec(),
        }
    }

    fn submit_body(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","polling_url":"https://api.bfl.ai/v1/get_result?id={id}","cost":1.25,"input_mp":1.0,"output_mp":1.0}}"#
        )
    }

    fn png() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/pixel.png"
        ))
        .unwrap()
    }

    fn request() -> JobRequest {
        JobRequest {
            model_id: "flux-3-image".into(),
            compiled_prompt: "a red square".into(),
            params: std::collections::BTreeMap::new(),
            images: Vec::new(),
            box_snapshot: Vec::new(),
            created_at: "2026-10-15T00:00:00Z".into(),
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "easel-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    struct Harness {
        runner: gpui::Entity<JobRunner>,
        transport: Arc<FakeTransport>,
        history: Arc<HistoryStore>,
        root: std::path::PathBuf,
    }

    fn open(
        cx: &mut TestAppContext,
        name: &str,
        script: Vec<Result<HttpResponse, TransportError>>,
    ) -> Harness {
        let transport = Arc::new(FakeTransport::scripted(script));
        let keys: Arc<dyn KeyResolver> =
            Arc::new(MapKeys::default().insert(ProviderId::Bfl, "bfl-test-key-xyz"));
        let root = temp_dir(name);
        let history = Arc::new(HistoryStore::open(&root).unwrap());
        let runner = cx.update({
            let transport = transport.clone() as Arc<dyn HttpTransport>;
            let history = history.clone();
            move |cx| {
                cx.new(|_| {
                    JobRunner::new(
                        easel_core::providers::resolve_model,
                        transport,
                        keys,
                        history,
                    )
                })
            }
        });
        Harness {
            runner,
            transport,
            history,
            root,
        }
    }

    fn open_without_key(
        cx: &mut TestAppContext,
        script: Vec<Result<HttpResponse, TransportError>>,
    ) -> Harness {
        let transport = Arc::new(FakeTransport::scripted(script));
        let keys: Arc<dyn KeyResolver> = Arc::new(MapKeys::default());
        let root = temp_dir("no-key");
        let history = Arc::new(HistoryStore::open(&root).unwrap());
        let runner = cx.update({
            let transport = transport.clone() as Arc<dyn HttpTransport>;
            let history = history.clone();
            move |cx| {
                cx.new(|_| {
                    JobRunner::new(
                        easel_core::providers::resolve_model,
                        transport,
                        keys,
                        history,
                    )
                })
            }
        });
        Harness {
            runner,
            transport,
            history,
            root,
        }
    }

    #[gpui::test]
    fn the_snapshotted_model_posts_to_its_own_host(cx: &mut TestAppContext) {
        use serde_json::json;

        let transport = Arc::new(FakeTransport::scripted(Vec::new()));
        let keys: Arc<dyn KeyResolver> = Arc::new(
            MapKeys::default()
                .insert(ProviderId::Bfl, "bfl-test-key-xyz")
                .insert(ProviderId::Krea, "krea-test-key")
                .insert(ProviderId::Google, "gemini-test-key"),
        );
        let history = Arc::new(HistoryStore::open(temp_dir("hosts")).unwrap());
        let runner = cx.update({
            let transport = transport.clone() as Arc<dyn HttpTransport>;
            move |cx| {
                cx.new(|_| {
                    JobRunner::new(
                        easel_core::providers::resolve_model,
                        transport,
                        keys,
                        history,
                    )
                })
            }
        });
        let mut google = request();
        google.model_id = "gemini-nano-banana-2.1".into();
        google.params.insert("resolution".into(), json!("2K"));
        google.params.insert("aspect_ratio".into(), json!("16:9"));
        google.images.push(EncodedImage {
            media_type: "image/png".into(),
            bytes: png(),
            width: 16,
            height: 16,
            content_hash: "pixel".into(),
        });
        runner.update(cx, |runner, cx| runner.submit(google, cx));
        cx.run_until_parked();
        let seen = transport.seen();
        assert_eq!(
            seen[0].url,
            "https://generativelanguage.googleapis.com/v1beta/interactions"
        );
        let easel_core::http::Body::Json(body) = &seen[0].body else {
            panic!("expected json");
        };
        assert_eq!(body["response_format"]["image_size"], json!("2K"));
        assert_eq!(body["response_format"]["aspect_ratio"], json!("16:9"));
        assert_eq!(body["store"], json!(false));
        assert!(body.get("aspect_ratio").is_none());

        let mut flux = request();
        flux.model_id = "flux-3-image".into();
        runner.update(cx, |runner, cx| runner.submit(flux, cx));
        cx.run_until_parked();
        assert!(
            transport.seen()[1].url.starts_with("https://api.bfl.ai/"),
            "{}",
            transport.seen()[1].url
        );

        let mut krea = request();
        krea.model_id = "krea-3".into();
        runner.update(cx, |runner, cx| runner.submit(krea, cx));
        cx.run_until_parked();
        assert!(
            transport.seen()[2].url.starts_with("https://api.krea.ai/"),
            "{}",
            transport.seen()[2].url
        );

        let before = transport.seen().len();
        let mut unknown = request();
        unknown.model_id = "no-such-model".into();
        runner.update(cx, |runner, cx| runner.submit(unknown, cx));
        cx.run_until_parked();
        assert_eq!(transport.seen().len(), before);
        let states = runner.read_with(cx, |runner, _| runner.states());
        assert!(
            states
                .iter()
                .any(|state| matches!(state, JobState::Failed { .. })),
            "{states:?}"
        );
    }

    fn open_google(
        cx: &mut TestAppContext,
        script: Vec<Result<HttpResponse, TransportError>>,
    ) -> Harness {
        let transport = Arc::new(FakeTransport::scripted(script));
        let keys: Arc<dyn KeyResolver> = Arc::new(
            MapKeys::default()
                .insert(ProviderId::Bfl, "bfl-test-key-xyz")
                .insert(ProviderId::Google, "gemini-test-key"),
        );
        let root = temp_dir("google-job");
        let history = Arc::new(HistoryStore::open(&root).unwrap());
        let runner = cx.update({
            let transport = transport.clone() as Arc<dyn HttpTransport>;
            let history = history.clone();
            move |cx| {
                cx.new(|_| {
                    JobRunner::new(
                        easel_core::providers::resolve_model,
                        transport,
                        keys,
                        history,
                    )
                })
            }
        });
        Harness {
            runner,
            transport,
            history,
            root,
        }
    }

    fn google_request() -> JobRequest {
        let mut request = request();
        request.model_id = "gemini-nano-banana-2.1".into();
        request
    }

    fn image_response(mime: &str, bytes: &[u8], interaction: &str) -> String {
        let data = EncodedImage {
            media_type: mime.into(),
            bytes: bytes.to_vec(),
            width: 1,
            height: 1,
            content_hash: String::new(),
        }
        .base64();
        format!(
            r#"{{"id":"{interaction}","output_image":{{"mime_type":"{mime}","data":"{data}"}}}}"#
        )
    }

    #[gpui::test]
    fn a_google_png_lands_in_history_without_a_poll(cx: &mut TestAppContext) {
        let bytes = png();
        let interaction = "x".repeat(70);
        let harness = open_google(
            cx,
            vec![Ok(http(
                200,
                image_response("image/png", &bytes, &interaction),
            ))],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(google_request(), cx));
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), 1);
        assert_eq!(harness.transport.seen()[0].method, Method::Post);
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
        let listed = harness.history.list().unwrap();
        let id = listed[0].id.as_str();
        assert_ne!(id, interaction);
        assert_eq!(
            std::fs::read(harness.root.join(id).join("result.png")).unwrap(),
            bytes
        );
        let meta: serde_json::Value = serde_json::from_slice(
            &std::fs::read(harness.root.join(id).join("meta.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta["interaction_id"], interaction);
        assert!(meta["poll_url"].is_null());
        assert_eq!(meta["provider"], "google");
    }

    #[gpui::test]
    fn a_steps_response_saves_the_model_image_and_the_search_suggestion(cx: &mut TestAppContext) {
        let thought = b"\x89PNG\r\n\x1a\nthought".to_vec();
        let bytes = png();
        let thought_data = EncodedImage {
            media_type: "image/png".into(),
            bytes: thought,
            width: 1,
            height: 1,
            content_hash: String::new(),
        }
        .base64();
        let data = EncodedImage {
            media_type: "image/png".into(),
            bytes: bytes.clone(),
            width: 1,
            height: 1,
            content_hash: String::new(),
        }
        .base64();
        let suggestion = "<div class='search'>cats</div>";
        let body = format!(
            r#"{{"id":"interaction-1","status":"completed","steps":[{{"type":"thought","content":[{{"type":"image","mime_type":"image/png","data":"{thought_data}"}}]}},{{"type":"google_search_result","result":[{{"search_suggestions":"{suggestion}"}}]}},{{"type":"model_output","content":[{{"type":"text","text":"a golf ball"}},{{"type":"image","mime_type":"image/png","data":"{data}"}}]}}]}}"#
        );
        let harness = open_google(cx, vec![Ok(http(200, body))]);
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(google_request(), cx));
        cx.run_until_parked();
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
        let id = harness.history.list().unwrap()[0].id.clone();
        assert_eq!(
            std::fs::read(harness.root.join(id.as_str()).join("result.png")).unwrap(),
            bytes
        );
        assert_eq!(harness.history.suggestion(&id), suggestion);
        assert!(
            harness
                .transport
                .seen()
                .iter()
                .all(|request| !request.url.contains("cite.example"))
        );
    }

    #[gpui::test]
    fn a_google_jpeg_keeps_the_decoded_bytes(cx: &mut TestAppContext) {
        let bytes = b"\xff\xd8\xffjpeg".to_vec();
        let harness = open_google(
            cx,
            vec![Ok(http(200, image_response("image/jpeg", &bytes, "short")))],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(google_request(), cx));
        cx.run_until_parked();
        let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
        assert_eq!(
            std::fs::read(harness.root.join(&id).join("result.jpg")).unwrap(),
            bytes
        );
    }

    #[gpui::test]
    fn a_grounded_response_stores_the_suggestion_and_does_not_fetch_the_citation(
        cx: &mut TestAppContext,
    ) {
        let bytes = png();
        let data = EncodedImage {
            media_type: "image/png".into(),
            bytes,
            width: 1,
            height: 1,
            content_hash: String::new(),
        }
        .base64();
        let suggestion = "<div class='search'>cats</div>";
        let body = format!(
            r#"{{"id":"grounded","output_image":{{"mime_type":"image/png","data":"{data}"}},"search_suggestions":"{suggestion}","url_citation":[{{"url":"https://cite.example/a"}}]}}"#
        );
        let harness = open_google(cx, vec![Ok(http(200, body))]);
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(google_request(), cx));
        cx.run_until_parked();
        assert!(
            harness
                .transport
                .seen()
                .iter()
                .all(|request| !request.url.contains("cite.example"))
        );
        let id = harness.history.list().unwrap()[0].id.clone();
        assert_eq!(harness.history.suggestion(&id), suggestion);
    }

    #[gpui::test]
    fn google_safety_and_auth_failures_write_a_row_and_no_image(cx: &mut TestAppContext) {
        for (status, body, line) in [
            (
                400,
                r#"{"error":{"status":"image_safety"}}"#,
                "blocked by safety",
            ),
            (
                400,
                r#"{"error":{"status":"image_prohibited_content"}}"#,
                "blocked by safety",
            ),
            (
                401,
                r#"{"error":{"status":"UNAUTHENTICATED"}}"#,
                "authentication failed",
            ),
            (
                404,
                r#"{"error":{"status":"model_not_found"}}"#,
                "model not found",
            ),
        ] {
            let harness = open_google(cx, vec![Ok(http(status, body))]);
            harness
                .runner
                .update(cx, |runner, cx| runner.submit(google_request(), cx));
            cx.run_until_parked();
            assert_eq!(harness.transport.seen().len(), 1, "{line}");
            assert_eq!(harness.transport.seen()[0].method, Method::Post);
            let listed = harness.history.list().unwrap();
            let JobState::Failed { error } = &listed[0].state else {
                panic!("{line} {:?}", listed);
            };
            assert_eq!(error.to_string(), line);
            let id = listed[0].id.as_str();
            assert!(harness.root.join(id).join("request.json").is_file());
            assert!(harness.root.join(id).join("meta.json").is_file());
            assert!(
                std::fs::read_dir(harness.root.join(id))
                    .unwrap()
                    .all(|entry| {
                        let name = entry.unwrap().file_name();
                        let name = name.to_string_lossy();
                        !name.starts_with("result.")
                    })
            );
            let app = cx.update(|cx| cx.new(|_| crate::app_state::AppState::new()));
            let strip = cx.update({
                let runner = harness.runner.clone();
                let history = harness.history.clone();
                move |cx| {
                    cx.new(|cx| {
                        crate::bottom_strip::BottomStrip::new(app, Some(runner), history, cx)
                    })
                }
            });
            strip.update(cx, |strip, cx| strip.refresh(cx));
            let lines = strip.read_with(cx, |strip, _| strip.lines());
            assert!(lines.iter().any(|text| text == line), "{lines:?}");
        }
    }

    #[gpui::test]
    fn dismiss_during_the_google_post_writes_nothing(cx: &mut TestAppContext) {
        let harness = open_google(
            cx,
            vec![Ok(http(200, image_response("image/png", &png(), "late")))],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(google_request(), cx));
        harness
            .runner
            .update(cx, |runner, cx| runner.dismiss(0, cx));
        cx.run_until_parked();
        assert!(harness.history.list().unwrap().is_empty());
    }

    #[gpui::test]
    fn resume_fails_an_in_flight_google_row_without_http(cx: &mut TestAppContext) {
        let harness = open_google(cx, Vec::new());
        let id = easel_core::providers::JobId::parse("resume-google").unwrap();
        harness
            .history
            .begin(
                &id,
                &google_request(),
                &easel_core::providers::Submitted {
                    job_id: id.clone(),
                    poll_url: "https://generativelanguage.googleapis.com/v1beta/interactions"
                        .parse()
                        .unwrap(),
                    quote: None,
                },
                None,
            )
            .unwrap();
        harness.runner.update(cx, |runner, cx| runner.resume(cx));
        cx.run_until_parked();
        assert!(harness.transport.seen().is_empty());
        let listed = harness.history.list().unwrap();
        assert!(matches!(listed[0].state, JobState::Failed { .. }));
    }

    #[gpui::test]
    fn resume_rejects_a_bfl_row_whose_poll_host_is_krea(cx: &mut TestAppContext) {
        let harness = open(cx, "bfl-krea-host", Vec::new());
        let id = easel_core::providers::JobId::parse("resume-bfl").unwrap();
        harness
            .history
            .begin(
                &id,
                &request(),
                &easel_core::providers::Submitted {
                    job_id: id.clone(),
                    poll_url: "https://api.krea.ai/jobs/job-1".parse().unwrap(),
                    quote: None,
                },
                None,
            )
            .unwrap();
        harness.runner.update(cx, |runner, cx| runner.resume(cx));
        cx.run_until_parked();
        assert!(harness.transport.seen().is_empty());
        let listed = harness.history.list().unwrap();
        assert!(matches!(listed[0].state, JobState::Failed { .. }));
    }

    #[gpui::test]
    fn submit_poll_and_download_reach_done(cx: &mut TestAppContext) {
        let ready = r#"{"status":"Ready","result":{"sample":"https://delivery.bfl.ai/img.png","prompt":"expanded","duration":3.5},"cost":4.5}"#;
        let harness = open(
            cx,
            "done",
            vec![
                Ok(http(200, submit_body("job1"))),
                Ok(http(200, r#"{"status":"Pending"}"#)),
                Ok(http(200, r#"{"status":"Generating"}"#)),
                Ok(http(200, ready)),
                Ok(http(200, png())),
            ],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), 1);
        cx.executor().advance_clock(Duration::from_millis(999));
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), 1);
        cx.executor().advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), 2);
        cx.executor().advance_clock(Duration::from_millis(1500));
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), 3);
        cx.executor().advance_clock(Duration::from_millis(2250));
        cx.run_until_parked();
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
        assert_eq!(harness.history.list().unwrap().len(), 1);
        let seen = harness.transport.seen();
        assert_eq!(seen[0].auth, Some(ProviderId::Bfl));
        assert_eq!(seen[1].auth, Some(ProviderId::Bfl));
        assert!(
            seen.iter()
                .any(|request| request.auth.is_none() && request.url.contains("delivery"))
        );
        let dumped = format!("{seen:?}");
        assert!(!dumped.contains("bfl-test-key-xyz"));
    }

    #[gpui::test]
    fn moderated_response_fails_without_a_download(cx: &mut TestAppContext) {
        let harness = open(
            cx,
            "moderated",
            vec![
                Ok(http(200, submit_body("job1"))),
                Ok(http(
                    200,
                    r#"{"status":"Request Moderated","details":{"Moderation Reasons":["Sexual","Violence"]}}"#,
                )),
            ],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        match &states[0] {
            JobState::Failed {
                error: ProviderError::Moderated { reasons },
            } => {
                assert!(reasons.iter().any(|reason| reason == "Sexual"));
            }
            other => panic!("{other:?}"),
        }
        assert!(
            harness
                .transport
                .seen()
                .iter()
                .all(|request| !request.url.contains("delivery"))
        );
    }

    #[gpui::test]
    fn polling_stops_at_the_deadline(cx: &mut TestAppContext) {
        let mut script = vec![Ok(http(200, submit_body("job1")))];
        script.extend(std::iter::repeat_n(
            Ok(http(200, r#"{"status":"Pending"}"#)),
            80,
        ));
        let harness = open(cx, "deadline", script);
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(5 * 60));
        cx.run_until_parked();
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        let JobState::Failed { error } = &states[0] else {
            panic!("{states:?}");
        };
        assert!(
            error.to_string().to_ascii_lowercase().contains("timed out"),
            "{error}"
        );
        let polls = harness.transport.seen().len();
        cx.executor().advance_clock(Duration::from_secs(30));
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), polls);
    }

    #[gpui::test]
    fn a_refused_poll_keeps_the_backoff(cx: &mut TestAppContext) {
        let harness = open(
            cx,
            "backoff",
            vec![
                Ok(http(200, submit_body("job1"))),
                Ok(http(200, r#"{"status":"Pending"}"#)),
                Err(TransportError::Network {
                    method: "GET".into(),
                    url: "https://api.bfl.ai/v1/get_result?id=job1".into(),
                }),
                Ok(http(200, r#"{"status":"Generating"}"#)),
                Ok(http(
                    200,
                    r#"{"status":"Ready","result":{"sample":"https://delivery.bfl.ai/img.png"},"cost":1.0}"#,
                )),
                Ok(http(200, png())),
            ],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(1500));
        cx.run_until_parked();
        let after_refusal = harness.transport.seen().len();
        cx.executor().advance_clock(Duration::from_millis(900));
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), after_refusal);
        cx.executor().advance_clock(Duration::from_millis(1400));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(3400));
        cx.run_until_parked();
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    }

    #[gpui::test]
    fn dismiss_stops_polling_and_a_queued_job_sends_nothing(cx: &mut TestAppContext) {
        let mut script = Vec::new();
        for id in ["job1", "job2", "job3", "job4"] {
            script.push(Ok(http(200, submit_body(id))));
        }
        let harness = open(cx, "dismiss", script);
        harness.runner.update(cx, |runner, cx| {
            for _ in 0..5 {
                runner.submit(request(), cx);
            }
        });
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), 4);
        let queued = harness.runner.read_with(cx, |runner, _| {
            runner
                .states()
                .iter()
                .filter(|state| matches!(state, JobState::Queued))
                .count()
        });
        assert_eq!(queued, 1);
        harness
            .runner
            .update(cx, |runner, cx| runner.dismiss(4, cx));
        assert_eq!(harness.transport.seen().len(), 4);
        harness
            .runner
            .update(cx, |runner, cx| runner.dismiss(0, cx));
        let before = harness.transport.seen().len();
        cx.executor().advance_clock(Duration::from_secs(10));
        cx.run_until_parked();
        assert!(
            harness.transport.seen()[before..]
                .iter()
                .all(|request| !request.url.contains("job1"))
        );
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(
            states
                .iter()
                .any(|state| matches!(state, JobState::Dismissed))
        );
    }

    #[gpui::test]
    fn safety_validation_keeps_the_approved_max(cx: &mut TestAppContext) {
        let harness = open(
            cx,
            "safety",
            vec![Ok(http(
                422,
                r#"{"detail":[{"loc":["body","safety_tolerance"],"msg":"Approved levels: 0, 1, 2, 3, 4","type":"value_error"}]}"#,
            ))],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        match &states[0] {
            JobState::Failed {
                error: ProviderError::Validation { field_errors, .. },
            } => assert_eq!(field_errors[0].approved_max, Some(4)),
            other => panic!("{other:?}"),
        }
    }

    #[gpui::test]
    fn a_forbidden_download_expires(cx: &mut TestAppContext) {
        let harness = open(
            cx,
            "expired",
            vec![
                Ok(http(200, submit_body("job1"))),
                Ok(http(
                    200,
                    r#"{"status":"Ready","result":{"sample":"https://delivery.bfl.ai/img.png?sig=secret"},"cost":1.0}"#,
                )),
                Ok(http(403, "gone")),
            ],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(matches!(states[0], JobState::Expired), "{states:?}");
        let meta = std::fs::read_to_string(harness.root.join("job1").join("meta.json")).unwrap();
        assert!(!meta.contains("delivery."));
        assert!(!meta.contains("sig="));
    }

    #[gpui::test]
    fn missing_key_fails_before_http(cx: &mut TestAppContext) {
        let harness = open_without_key(cx, Vec::new());
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        assert!(harness.transport.seen().is_empty());
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Auth
            }
        ));
    }

    #[gpui::test]
    fn an_off_list_poll_url_fails_resume_without_a_request(cx: &mut TestAppContext) {
        let harness = open(
            cx,
            "resume-bad",
            vec![
                Ok(http(200, submit_body("job1"))),
                Ok(http(200, r#"{"status":"Pending"}"#)),
            ],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        let meta_path = harness.root.join("job1").join("meta.json");
        let mut meta: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta["poll_url"] = serde_json::Value::String("https://evil.example/poll".into());
        std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();
        let before = harness.transport.seen().len();
        let history = harness.history.clone();
        let transport = harness.transport.clone() as Arc<dyn HttpTransport>;
        let keys: Arc<dyn KeyResolver> =
            Arc::new(MapKeys::default().insert(ProviderId::Bfl, "bfl-test-key-xyz"));
        let runner = cx.update(move |cx| {
            cx.new(|_| {
                JobRunner::new(
                    easel_core::providers::resolve_model,
                    transport,
                    keys,
                    history,
                )
            })
        });
        runner.update(cx, |runner, cx| runner.resume(cx));
        cx.run_until_parked();
        assert_eq!(harness.transport.seen().len(), before);
        let listed = harness.history.list().unwrap();
        assert!(
            listed
                .iter()
                .any(|entry| matches!(entry.state, JobState::Failed { .. }))
        );
    }

    #[derive(Clone, Copy)]
    struct UploadFlux;

    impl Provider for UploadFlux {
        fn spec(&self) -> &easel_core::model_spec::ModelSpec {
            Flux3.spec()
        }
        fn validate_for_submit(
            &self,
            scene: &easel_core::scene::Scene,
            values: &std::collections::BTreeMap<String, serde_json::Value>,
            image_count: usize,
        ) -> Vec<easel_core::error::FieldError> {
            Flux3.validate_for_submit(scene, values, image_count)
        }
        fn reference_steps(&self, images: &[EncodedImage]) -> Vec<ReferenceStep> {
            images
                .iter()
                .map(|image| ReferenceStep::Upload {
                    request: HttpRequest {
                        method: Method::Post,
                        url: "https://api.bfl.ai/v1/upload".into(),
                        headers: Vec::new(),
                        auth: Some(ProviderId::Bfl),
                        body: Body::Empty,
                        max_body_bytes: 1024,
                    },
                    content_hash: image.content_hash.clone(),
                })
                .collect()
        }
        fn parse_upload(&self, response: &HttpResponse) -> Result<ImageInput, ProviderError> {
            Ok(ImageInput::Url(
                String::from_utf8_lossy(&response.body).into(),
            ))
        }
        fn build_submit(&self, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
            Flux3.build_submit(request, images)
        }
        fn poll_request(&self, submitted: &easel_core::providers::Submitted) -> HttpRequest {
            Flux3.poll_request(submitted)
        }
        fn parse_submit(
            &self,
            response: &HttpResponse,
        ) -> Result<easel_core::providers::Submitted, ProviderError> {
            Flux3.parse_submit(response)
        }
        fn parse_poll(&self, response: &HttpResponse) -> easel_core::providers::PollOutcome {
            Flux3.parse_poll(response)
        }
    }

    #[gpui::test]
    fn an_upload_step_is_cached_by_content_hash(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![
            Ok(http(200, "https://api.bfl.ai/blob")),
            Ok(http(200, submit_body("job1"))),
            Ok(http(200, submit_body("job2"))),
        ]));
        let keys: Arc<dyn KeyResolver> =
            Arc::new(MapKeys::default().insert(ProviderId::Bfl, "bfl-test-key-xyz"));
        let history = Arc::new(HistoryStore::open(temp_dir("upload")).unwrap());
        let runner = cx.update({
            let transport = transport.clone() as Arc<dyn HttpTransport>;
            move |cx| {
                cx.new(|_| {
                    JobRunner::new(
                        |_| Some(Box::new(UploadFlux) as Box<dyn Provider>),
                        transport,
                        keys,
                        history,
                    )
                })
            }
        });
        let mut image_request = request();
        image_request.images.push(EncodedImage {
            media_type: "image/png".into(),
            bytes: png(),
            width: 1,
            height: 1,
            content_hash: "same-hash".into(),
        });
        let second = image_request.clone();
        runner.update(cx, |runner, cx| runner.submit(image_request, cx));
        cx.run_until_parked();
        runner.update(cx, |runner, cx| runner.submit(second, cx));
        cx.run_until_parked();
        let uploads = transport
            .seen()
            .iter()
            .filter(|request| request.url.contains("upload"))
            .count();
        assert_eq!(uploads, 1);
    }

    #[gpui::test]
    fn four_jobs_leave_a_consistent_history(cx: &mut TestAppContext) {
        let ready = r#"{"status":"Ready","result":{"sample":"https://delivery.bfl.ai/img.png"},"cost":1.0}"#;
        let mut script = Vec::new();
        for id in ["job1", "job2", "job3", "job4"] {
            script.push(Ok(http(200, submit_body(id))));
        }
        for _ in 0..4 {
            script.push(Ok(http(200, ready)));
        }
        for _ in 0..4 {
            script.push(Ok(http(200, png())));
        }
        let harness = open(cx, "four", script);
        harness.runner.update(cx, |runner, cx| {
            for _ in 0..4 {
                runner.submit(request(), cx);
            }
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(harness.history.list().unwrap().len(), 4);
    }

    #[gpui::test]
    fn an_oversized_result_fails_closed(cx: &mut TestAppContext) {
        let huge = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/huge.png"
        ))
        .unwrap();
        let harness = open(
            cx,
            "huge",
            vec![
                Ok(http(200, submit_body("job1"))),
                Ok(http(
                    200,
                    r#"{"status":"Ready","result":{"sample":"https://delivery.bfl.ai/img.png"},"cost":1.0}"#,
                )),
                Ok(http(200, huge)),
            ],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        let JobState::Failed { error } = &states[0] else {
            panic!("{states:?}");
        };
        assert!(error.to_string().contains("oversized"), "{error}");
    }

    #[gpui::test]
    fn a_failed_history_write_keeps_the_bytes(cx: &mut TestAppContext) {
        let harness = open(
            cx,
            "io",
            vec![
                Ok(http(200, submit_body("job1"))),
                Ok(http(
                    200,
                    r#"{"status":"Ready","result":{"sample":"https://delivery.bfl.ai/img.png"},"cost":1.0}"#,
                )),
                Ok(http(200, png())),
            ],
        );
        harness
            .runner
            .update(cx, |runner, cx| runner.submit(request(), cx));
        cx.run_until_parked();
        harness.history.fail_next_index_rename();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        let (state, bytes) = harness
            .runner
            .read_with(cx, |runner, _| (runner.states(), runner.result_bytes(0)));
        assert!(matches!(state[0], JobState::Failed { .. }), "{state:?}");
        assert!(bytes.is_some());
    }
}
