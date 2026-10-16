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
    submitted: Option<Submitted>,
    elapsed: Duration,
    bytes: Option<Vec<u8>>,
    dead: bool,
}

pub struct JobRunner<P> {
    provider: P,
    transport: Arc<dyn HttpTransport>,
    keys: Arc<dyn KeyResolver>,
    history: Arc<HistoryStore>,
    slots: Vec<Slot>,
    uploads: HashMap<String, ImageInput>,
    next_token: u64,
}

impl<P: Provider + 'static> JobRunner<P> {
    pub fn new(
        provider: P,
        transport: Arc<dyn HttpTransport>,
        keys: Arc<dyn KeyResolver>,
        history: Arc<HistoryStore>,
    ) -> Self {
        Self {
            provider,
            transport,
            keys,
            history,
            slots: Vec::new(),
            uploads: HashMap::new(),
            next_token: 1,
        }
    }

    pub fn submit(&mut self, request: JobRequest, cx: &mut Context<Self>) {
        let token = self.next_token;
        self.next_token += 1;
        self.slots.push(Slot {
            token,
            state: JobState::Queued,
            request,
            submitted: None,
            elapsed: Duration::ZERO,
            bytes: None,
            dead: false,
        });
        self.pump(cx);
    }

    pub fn dismiss(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        if matches!(slot.state, JobState::Queued) {
            self.slots.remove(index);
            return;
        }
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
        self.pump(cx);
    }

    pub fn states(&self) -> Vec<JobState> {
        self.slots.iter().map(|slot| slot.state.clone()).collect()
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
            if !ProviderId::Bfl.allows_url(&poll_url) {
                let error = ProviderError::Protocol("unexpected result host".into());
                let _ = self.history.fail(&entry.id, &error);
                continue;
            }
            let Ok(url) = poll_url.parse() else {
                continue;
            };
            let token = self.next_token;
            self.next_token += 1;
            self.slots.push(Slot {
                token,
                state: JobState::Polling {
                    attempt: 0,
                    progress: None,
                },
                request: empty_request(),
                submitted: Some(Submitted {
                    job_id: entry.id,
                    poll_url: url,
                    quote: None,
                }),
                elapsed: Duration::ZERO,
                bytes: None,
                dead: false,
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
        let provider_id = self.provider.spec().provider;
        if self.keys.key_for(provider_id).is_none() {
            self.fail(token, ProviderError::Auth, cx);
            return;
        }
        let images = self
            .slot_mut(token)
            .map(|slot| slot.request.images.clone())
            .unwrap_or_default();
        let mut steps = self.provider.reference_steps(&images);
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
                    if let Some(cached) = self.uploads.get(&content_hash).cloned() {
                        inputs.push(cached);
                        continue;
                    }
                    let task = self.transport.request(request, cx);
                    let rest = steps.clone();
                    let inputs = inputs.clone();
                    cx.spawn(async move |this, cx| {
                        let response = task.await;
                        this.update(cx, |runner, cx| {
                            runner.finish_upload(token, content_hash, response, rest, inputs, cx);
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
        match self.provider.parse_upload(&response) {
            Ok(input) => {
                self.uploads.insert(content_hash, input.clone());
                inputs.push(input);
                self.resolve_steps(token, &mut steps, &mut inputs, cx);
            }
            Err(error) => self.fail(token, error, cx),
        }
    }

    fn send_submit(&mut self, token: u64, inputs: Vec<ImageInput>, cx: &mut Context<Self>) {
        let Some(request) = self.slot_mut(token).map(|slot| slot.request.clone()) else {
            return;
        };
        let http = self.provider.build_submit(&request, &inputs);
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
        match self.provider.parse_submit(&response) {
            Ok(submitted) => {
                let request = self.slot_mut(token).map(|slot| slot.request.clone());
                if let Some(request) = request {
                    let _ = self.history.begin(&submitted.job_id, &request, &submitted);
                }
                if let Some(slot) = self.slot_mut(token) {
                    let state = std::mem::replace(&mut slot.state, JobState::Queued);
                    slot.state = advance(state, JobEvent::Submitted(submitted.clone()));
                    slot.submitted = Some(submitted);
                }
                self.schedule_poll(token, cx);
            }
            Err(error) => self.fail(token, error, cx),
        }
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
        let policy = self.provider.poll_policy();
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
        let policy = self.provider.poll_policy();
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
        let http = self.provider.poll_request(&submitted);
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
        let outcome = match response {
            Ok(response) => self.provider.parse_poll(&response),
            Err(TransportError::Network { .. } | TransportError::Timeout { .. }) => {
                PollOutcome::Failed(ProviderError::Network("poll failed".into()))
            }
            Err(error) => {
                PollOutcome::Failed(ProviderError::Network(strip_url(&error.to_string())))
            }
        };
        let downloading = matches!(outcome, PollOutcome::Ready { .. });
        if let Some(slot) = self.slot_mut(token) {
            let state = std::mem::replace(&mut slot.state, JobState::Queued);
            slot.state = advance(state, JobEvent::Poll(outcome.clone()));
        }
        match self.slot_mut(token).map(|slot| slot.state.clone()) {
            Some(JobState::Polling { .. }) => self.schedule_poll(token, cx),
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
        let http = self.provider.download_request(&url);
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
        let remaining = self.provider.poll_policy().deadline.saturating_sub(elapsed);
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
        runner: gpui::Entity<JobRunner<Flux3>>,
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
            move |cx| cx.new(|_| JobRunner::new(Flux3, transport, keys, history))
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
            move |cx| cx.new(|_| JobRunner::new(Flux3, transport, keys, history))
        });
        Harness {
            runner,
            transport,
            history,
            root,
        }
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
        let runner =
            cx.update(move |cx| cx.new(|_| JobRunner::new(Flux3, transport, keys, history)));
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
            move |cx| cx.new(|_| JobRunner::new(UploadFlux, transport, keys, history))
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
