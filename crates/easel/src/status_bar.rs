use std::sync::Arc;
use std::time::Duration;

use easel_core::http::HttpResponse;
use easel_core::job::JobState;
use easel_core::model_spec::ProviderId;
use easel_core::providers::bfl::credits::{credits_request, parse_credits};
use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb};

use crate::app_state::AppState;
use crate::chrome;
use crate::jobs::{JobRunner, LiveJob};
use crate::net::{HttpTransport, KeyResolver, TransportError};
use crate::notifications::Notifications;
use crate::settings::SettingsPanel;
use crate::theme;

const REFRESH: Duration = Duration::from_secs(60);

/// One line across the bottom of the window: the current operation on the
/// left, the credit balance and the loaded image's pixel size on the right.
pub struct StatusBar {
    notifications: gpui::Entity<Notifications>,
    app_state: gpui::Entity<AppState>,
    provider: ProviderId,
    transport: Arc<dyn HttpTransport>,
    keys: Arc<dyn KeyResolver>,
    credits: Option<f64>,
    fetch_id: u64,
    saw_key: bool,
    in_flight: Vec<(u64, String)>,
}

impl StatusBar {
    pub fn new(
        notifications: gpui::Entity<Notifications>,
        app_state: gpui::Entity<AppState>,
        provider: ProviderId,
        transport: Arc<dyn HttpTransport>,
        keys: Arc<dyn KeyResolver>,
        runner: Option<gpui::Entity<JobRunner>>,
        settings: Option<gpui::Entity<SettingsPanel>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let bar = Self {
            notifications,
            app_state,
            provider,
            transport,
            keys,
            credits: None,
            fetch_id: 0,
            saw_key: false,
            in_flight: Vec::new(),
        };
        if bar.provider == ProviderId::Bfl {
            if let Some(runner) = runner {
                cx.observe(&runner, |this, runner, cx| {
                    let jobs = runner.read(cx).live_jobs();
                    this.note_jobs(&jobs, cx);
                })
                .detach();
            }
            if let Some(settings) = settings {
                cx.observe(&settings, |this, _, cx| this.poke(cx)).detach();
            }
            cx.spawn(async move |this, cx| {
                this.update(cx, |bar, cx| bar.refresh(cx)).ok();
                loop {
                    cx.background_executor().timer(REFRESH).await;
                    if this.update(cx, |bar, cx| bar.refresh(cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        bar
    }

    pub fn label(&self) -> Option<String> {
        if self.provider != ProviderId::Bfl {
            return None;
        }
        self.credits.map(format_credits)
    }

    #[cfg(test)]
    pub fn status_line(&self, cx: &gpui::App) -> String {
        self.notifications.read(cx).status().to_string()
    }

    fn poke(&mut self, cx: &mut Context<Self>) {
        if self.provider != ProviderId::Bfl {
            return;
        }
        if self.keys.key_for(ProviderId::Bfl).is_none() {
            self.saw_key = false;
            return;
        }
        if !self.saw_key {
            self.refresh(cx);
        }
    }

    fn note_jobs(&mut self, jobs: &[LiveJob], cx: &mut Context<Self>) {
        let next = in_flight_signature(jobs);
        if next == self.in_flight {
            return;
        }
        self.in_flight = next;
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.provider != ProviderId::Bfl || self.keys.key_for(ProviderId::Bfl).is_none() {
            return;
        }
        self.saw_key = true;
        self.fetch_id = self.fetch_id.wrapping_add(1);
        let fetch_id = self.fetch_id;
        let task = self.transport.request(credits_request(), cx);
        cx.spawn(async move |this, cx| {
            let response = task.await;
            this.update(cx, |bar, cx| bar.apply(fetch_id, response, cx))
                .ok();
        })
        .detach();
    }

    fn apply(
        &mut self,
        fetch_id: u64,
        response: Result<HttpResponse, TransportError>,
        cx: &mut Context<Self>,
    ) {
        if fetch_id != self.fetch_id {
            return;
        }
        let Ok(response) = response else {
            return;
        };
        let Ok(credits) = parse_credits(&response) else {
            return;
        };
        self.credits = Some(credits);
        cx.notify();
    }
}

fn in_flight_signature(jobs: &[LiveJob]) -> Vec<(u64, String)> {
    let mut rows: Vec<(u64, String)> = jobs
        .iter()
        .filter(|job| job.state.is_in_flight())
        .map(|job| {
            let progress = match &job.state {
                JobState::Polling {
                    progress: Some(progress),
                    ..
                } => format!("{}", (progress.clamp(0.0, 1.0) * 100.0).round() as i32),
                _ => String::new(),
            };
            let phase = match &job.state {
                JobState::Queued => "queued",
                JobState::Submitting => "submitting",
                JobState::Polling { .. } => "polling",
                JobState::Downloading => "downloading",
                _ => "other",
            };
            (job.token, format!("{phase}:{}:{progress}", job.status))
        })
        .collect();
    rows.sort_by_key(|(token, _)| *token);
    rows
}

fn format_credits(credits: f64) -> String {
    let cents = (credits * 100.0).round();
    let whole = cents / 100.0;
    let text = if cents % 100.0 == 0.0 {
        format!("{whole:.0}")
    } else if cents % 10.0 == 0.0 {
        format!("{whole:.1}")
    } else {
        format!("{whole:.2}")
    };
    format!("{text} credits")
}

impl Render for StatusBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.notifications.read(cx).status().clone();
        let image_size = self
            .app_state
            .read(cx)
            .image
            .as_ref()
            .map(|image| format!("{} × {}", image.size.w, image.size.h));
        let credits = self.label();
        let tiling = chrome::tiling(window);
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .justify_between()
            .w_full()
            .relative()
            .h(px(theme::STATUS_BAR_HEIGHT))
            .px(px(12.))
            .bg(rgb(theme::TITLE_BAR))
            .text_xs()
            .text_color(rgb(theme::TEXT_MUTED))
            .when(chrome::is_client_decorated(window), |bar| {
                chrome::round_client_corners(
                    bar,
                    gpui::Tiling {
                        top: true,
                        ..tiling
                    },
                )
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(theme::WINDOW_BORDER)
                    .bg(rgb(theme::BORDER)),
            )
            .child(div().flex_1().min_w_0().overflow_hidden().child(status))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(16.))
                    .children(credits)
                    .children(image_size),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use easel_core::http::HttpResponse;
    use easel_core::job::JobState;
    use easel_core::model_spec::ProviderId;
    use gpui::{AppContext as _, TestAppContext};

    use crate::app_state::AppState;
    use crate::jobs::LiveJob;
    use crate::net::{FakeTransport, MapKeys};
    use crate::notifications::Notifications;

    use super::{StatusBar, format_credits, in_flight_signature};

    fn http(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn job(token: u64, status: &str, state: JobState) -> LiveJob {
        LiveJob {
            token,
            id: None,
            state,
            created_at: String::new(),
            cost: None,
            status: status.into(),
            bytes: None,
            prose: None,
            model_id: String::new(),
            editor: None,
        }
    }

    fn open(
        cx: &mut TestAppContext,
        provider: ProviderId,
        transport: Arc<FakeTransport>,
        keys: Arc<MapKeys>,
    ) -> gpui::Entity<StatusBar> {
        let carried = Arc::clone(&transport);
        cx.update(|cx| {
            let notes = cx.new(|_| Notifications::new());
            let state = cx.new(|_| AppState::new());
            cx.new(|cx| StatusBar::new(notes, state, provider, carried, keys, None, None, cx))
        })
    }

    #[test]
    fn credit_text_drops_trailing_zeros() {
        assert_eq!(format_credits(42.0), "42 credits");
        assert_eq!(format_credits(12.5), "12.5 credits");
        assert_eq!(format_credits(8.25), "8.25 credits");
    }

    #[test]
    fn a_status_change_is_a_new_signature() {
        let pending = job(
            1,
            "Pending",
            JobState::Polling {
                attempt: 1,
                progress: None,
            },
        );
        let generating = job(
            1,
            "Generating",
            JobState::Polling {
                attempt: 2,
                progress: Some(0.4),
            },
        );
        assert_ne!(
            in_flight_signature(std::slice::from_ref(&pending)),
            in_flight_signature(std::slice::from_ref(&generating))
        );
        assert_eq!(
            in_flight_signature(std::slice::from_ref(&pending)),
            in_flight_signature(std::slice::from_ref(&pending))
        );
    }

    #[gpui::test]
    fn a_black_forest_model_shows_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Bfl, "test-key"));
        let bar = open(cx, ProviderId::Bfl, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(
            bar.read_with(cx, |bar, _| bar.label()),
            Some("42 credits".into())
        );
        assert_eq!(transport.seen().len(), 1);
        assert_eq!(transport.seen()[0].url, "https://api.bfl.ai/v1/credits");
    }

    #[gpui::test]
    fn credits_refresh_every_minute(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![
            Ok(http(r#"{"credits": 10}"#)),
            Ok(http(r#"{"credits": 9}"#)),
        ]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Bfl, "test-key"));
        let bar = open(cx, ProviderId::Bfl, Arc::clone(&transport), keys);
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(60));
        cx.run_until_parked();
        assert_eq!(transport.seen().len(), 2);
        assert_eq!(
            bar.read_with(cx, |bar, _| bar.label()),
            Some("9 credits".into())
        );
    }

    #[gpui::test]
    fn a_generation_status_change_refreshes_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![
            Ok(http(r#"{"credits": 10}"#)),
            Ok(http(r#"{"credits": 8.5}"#)),
            Ok(http(r#"{"credits": 1}"#)),
        ]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Bfl, "test-key"));
        let bar = open(cx, ProviderId::Bfl, Arc::clone(&transport), keys);
        cx.run_until_parked();
        let pending = job(
            1,
            "Pending",
            JobState::Polling {
                attempt: 1,
                progress: None,
            },
        );
        bar.update(cx, |bar, cx| {
            bar.note_jobs(std::slice::from_ref(&pending), cx)
        });
        cx.run_until_parked();
        assert_eq!(transport.seen().len(), 2);
        assert_eq!(
            bar.read_with(cx, |bar, _| bar.label()),
            Some("8.5 credits".into())
        );
        bar.update(cx, |bar, cx| {
            bar.note_jobs(std::slice::from_ref(&pending), cx)
        });
        cx.run_until_parked();
        assert_eq!(transport.seen().len(), 2);
    }

    #[gpui::test]
    fn another_provider_does_not_ask_for_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Krea, "test-key"));
        let bar = open(cx, ProviderId::Krea, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |bar, _| bar.label()), None);
        assert!(transport.seen().is_empty());
    }

    #[gpui::test]
    fn openai_does_not_ask_for_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::OpenAi, "test-key"));
        let bar = open(cx, ProviderId::OpenAi, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |bar, _| bar.label()), None);
        assert!(
            transport
                .seen()
                .iter()
                .all(|request| !request.url.contains("openai.com"))
        );
    }

    #[gpui::test]
    fn meta_does_not_ask_for_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Meta, "test-key"));
        let bar = open(cx, ProviderId::Meta, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |bar, _| bar.label()), None);
        assert!(
            transport
                .seen()
                .iter()
                .all(|request| !request.url.contains("api.meta.ai"))
        );
    }

    #[gpui::test]
    fn xai_does_not_ask_for_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Xai, "test-key"));
        let bar = open(cx, ProviderId::Xai, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |bar, _| bar.label()), None);
        assert!(
            transport
                .seen()
                .iter()
                .all(|request| !request.url.contains("api.x.ai"))
        );
    }

    #[gpui::test]
    fn chutes_does_not_ask_for_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Chutes, "test-key"));
        let bar = open(cx, ProviderId::Chutes, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |bar, _| bar.label()), None);
        assert!(transport.seen().is_empty(), "{:?}", transport.seen());
    }

    #[gpui::test]
    fn apiframe_does_not_ask_for_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Apiframe, "test-key"));
        let bar = open(cx, ProviderId::Apiframe, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |bar, _| bar.label()), None);
        assert!(transport.seen().is_empty(), "{:?}", transport.seen());
    }

    #[gpui::test]
    fn leonardo_does_not_ask_for_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Leonardo, "test-key"));
        let bar = open(cx, ProviderId::Leonardo, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |bar, _| bar.label()), None);
        assert!(transport.seen().is_empty(), "{:?}", transport.seen());
    }

    #[gpui::test]
    fn stability_does_not_ask_for_credits(cx: &mut TestAppContext) {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(http(
            r#"{"credits": 42}"#,
        ))]));
        let keys = Arc::new(MapKeys::default().insert(ProviderId::Stability, "test-key"));
        let bar = open(cx, ProviderId::Stability, Arc::clone(&transport), keys);
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |bar, _| bar.label()), None);
        assert!(transport.seen().is_empty(), "{:?}", transport.seen());
    }
}
