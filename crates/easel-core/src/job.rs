use crate::error::ProviderError;
use crate::providers::{PollOutcome, Submitted};
use crate::scene::BoxElement;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct PollPolicy {
    pub initial: Duration,
    pub factor: f64,
    pub cap: Duration,
    pub deadline: Duration,
}

impl PollPolicy {
    #[must_use]
    pub fn default_bfl() -> Self {
        Self {
            initial: Duration::from_secs(1),
            factor: 1.5,
            cap: Duration::from_secs(5),
            deadline: Duration::from_secs(5 * 60),
        }
    }

    #[must_use]
    pub fn next_delay(&self, attempt: u32) -> Duration {
        let mut seconds = self.initial.as_secs_f64();
        for _ in 0..attempt {
            seconds *= self.factor;
        }
        Duration::from_secs_f64(seconds.min(self.cap.as_secs_f64()))
    }

    #[must_use]
    pub fn expired(&self, elapsed: Duration) -> bool {
        elapsed >= self.deadline
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum JobEvent {
    SlotFreed,
    Submitted(Submitted),
    InlineReady {
        result_path: String,
    },
    Poll(PollOutcome),
    Downloaded {
        result_path: String,
        cost: Option<f64>,
    },
    DownloadExpired,
    DownloadFailed(ProviderError),
    Dismiss,
    Deadline,
}

#[must_use]
pub fn advance(state: JobState, event: JobEvent) -> JobState {
    if matches!(event, JobEvent::Dismiss) {
        return if state.is_in_flight() {
            JobState::Dismissed
        } else {
            state
        };
    }
    if matches!(event, JobEvent::Deadline) && state.is_in_flight() {
        return JobState::Failed {
            error: ProviderError::Protocol("Timed out waiting for result".into()),
        };
    }
    match (state, event) {
        (JobState::Queued, JobEvent::SlotFreed) => JobState::Submitting,
        (JobState::Submitting, JobEvent::InlineReady { result_path }) => JobState::Done {
            result_path,
            cost: None,
        },
        (JobState::Submitting | JobState::Queued, JobEvent::Submitted(_)) => JobState::Polling {
            attempt: 0,
            progress: None,
        },
        (JobState::Polling { attempt, progress }, JobEvent::Poll(outcome)) => {
            poll_transition(attempt, progress, outcome)
        }
        (JobState::Downloading, JobEvent::Downloaded { result_path, cost }) => {
            JobState::Done { result_path, cost }
        }
        (JobState::Downloading, JobEvent::DownloadExpired) => JobState::Expired,
        (JobState::Downloading, JobEvent::DownloadFailed(error)) => JobState::Failed { error },
        (state, _) => state,
    }
}

fn poll_transition(attempt: u32, progress: Option<f32>, outcome: PollOutcome) -> JobState {
    match outcome {
        PollOutcome::InFlight { progress: next, .. } => JobState::Polling {
            attempt: attempt.saturating_add(1),
            progress: next.or(progress),
        },
        PollOutcome::Ready { .. } => JobState::Downloading,
        PollOutcome::Cancelled => JobState::Dismissed,
        PollOutcome::Failed(ProviderError::Network(_)) => JobState::Polling {
            attempt: attempt.saturating_add(1),
            progress,
        },
        PollOutcome::Failed(error) => JobState::Failed { error },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodedImage {
    pub media_type: String,
    pub bytes: Vec<u8>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub content_hash: String,
}

/// A turn the provider was asked to keep. Absent means a one-shot generate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRequest {
    pub model_id: String,
    pub compiled_prompt: String,
    pub params: BTreeMap<String, Value>,
    pub images: Vec<EncodedImage>,
    pub box_snapshot: Vec<BoxElement>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<Conversation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Submitting,
    Polling {
        attempt: u32,
        progress: Option<f32>,
    },
    Downloading,
    Done {
        result_path: String,
        cost: Option<f64>,
    },
    Failed {
        error: ProviderError,
    },
    Dismissed,
    Expired,
}

impl JobState {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Done { .. } | Self::Failed { .. } | Self::Dismissed | Self::Expired
        )
    }

    #[must_use]
    pub fn is_in_flight(&self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Submitting | Self::Polling { .. } | Self::Downloading
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{EncodedImage, JobEvent, JobRequest, JobState, PollPolicy, advance};
    use crate::error::ProviderError;
    use crate::scene::{BoxElement, BoxId, BoxKind};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::Duration;

    fn request() -> JobRequest {
        JobRequest {
            model_id: "flux-3".into(),
            compiled_prompt: "a fox".into(),
            params: BTreeMap::from([("steps".into(), json!(20))]),
            images: vec![EncodedImage {
                media_type: "image/png".into(),
                bytes: vec![1, 2, 3],
                width: 1,
                height: 1,
                content_hash: "abc".into(),
            }],
            box_snapshot: vec![BoxElement {
                id: BoxId::parse("animal_1").unwrap(),
                kind: BoxKind::Keep,
                desc: "fox".into(),
                src: None,
                tgt: None,
                from_image: None,
            }],
            created_at: "2026-10-15T00:00:00Z".into(),
            conversation: None,
        }
    }

    fn states() -> Vec<JobState> {
        vec![
            JobState::Queued,
            JobState::Submitting,
            JobState::Polling {
                attempt: 2,
                progress: Some(0.5),
            },
            JobState::Downloading,
            JobState::Done {
                result_path: "out.png".into(),
                cost: Some(1.5),
            },
            JobState::Failed {
                error: ProviderError::RateLimited {
                    retry_after: Some(Duration::from_secs(3)),
                },
            },
            JobState::Dismissed,
            JobState::Expired,
        ]
    }

    #[test]
    fn job_request_and_every_state_round_trip() {
        let request = request();
        let json = serde_json::to_value(&request).unwrap();
        let back: JobRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request, back);

        for state in states() {
            let json = serde_json::to_value(&state).unwrap();
            let back: JobState = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(state, back);
        }
    }

    #[test]
    fn downloading_state_has_no_url() {
        let json = serde_json::to_value(JobState::Downloading)
            .unwrap()
            .to_string();
        assert!(!json.to_lowercase().contains("url"));
        assert!(!json.contains("http"));
    }

    #[test]
    fn poll_policy_backs_off_and_expires() {
        let policy = PollPolicy::default_bfl();
        let delays: Vec<_> = [0, 1, 2, 10]
            .into_iter()
            .map(|attempt| policy.next_delay(attempt).as_secs_f64())
            .collect();
        assert!((delays[0] - 1.0).abs() < 1e-6);
        assert!((delays[1] - 1.5).abs() < 1e-6);
        assert!((delays[2] - 2.25).abs() < 1e-6);
        assert!((delays[3] - 5.0).abs() < 1e-6);
        assert!(policy.expired(Duration::from_secs(5 * 60 + 1)));
        assert!(!policy.expired(Duration::from_secs(5 * 60 - 1)));
    }

    #[test]
    fn advance_follows_the_job_lifecycle() {
        use crate::providers::PollOutcome;

        let polling = JobState::Polling {
            attempt: 1,
            progress: Some(0.2),
        };
        assert!(matches!(
            advance(JobState::Queued, JobEvent::SlotFreed),
            JobState::Submitting
        ));
        assert!(matches!(
            advance(
                polling.clone(),
                JobEvent::Poll(PollOutcome::Ready {
                    url: "https://delivery.bfl.ai/x.png".parse().unwrap(),
                    settled_cost: None,
                    prompt: None,
                    duration: None,
                    grid_urls: None,
                })
            ),
            JobState::Downloading
        ));
        assert!(matches!(
            advance(polling.clone(), JobEvent::Poll(PollOutcome::Cancelled)),
            JobState::Dismissed
        ));
        match advance(
            polling.clone(),
            JobEvent::Poll(PollOutcome::Failed(ProviderError::Network("reset".into()))),
        ) {
            JobState::Polling { attempt, .. } => assert_eq!(attempt, 2),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            advance(
                polling.clone(),
                JobEvent::Poll(PollOutcome::Failed(ProviderError::Moderated {
                    reasons: vec!["Hate".into()],
                })),
            ),
            JobState::Failed {
                error: ProviderError::Moderated { .. }
            }
        ));
        assert!(matches!(
            advance(JobState::Downloading, JobEvent::DownloadExpired),
            JobState::Expired
        ));
        assert!(matches!(
            advance(
                JobState::Downloading,
                JobEvent::DownloadFailed(ProviderError::Protocol("too big".into())),
            ),
            JobState::Failed {
                error: ProviderError::Protocol(_)
            }
        ));
        match advance(polling.clone(), JobEvent::Deadline) {
            JobState::Failed { error } => {
                let text = error.to_string();
                assert!(text.to_ascii_lowercase().contains("timed out"));
            }
            other => panic!("{other:?}"),
        }
        for state in [
            JobState::Queued,
            JobState::Submitting,
            polling,
            JobState::Downloading,
        ] {
            assert!(matches!(
                advance(state, JobEvent::Dismiss),
                JobState::Dismissed
            ));
        }
        let done = JobState::Done {
            result_path: "out.png".into(),
            cost: None,
        };
        assert_eq!(advance(done.clone(), JobEvent::Dismiss), done);
    }

    #[test]
    fn terminal_and_in_flight_match_the_lifecycle() {
        for state in states() {
            let terminal = matches!(
                state,
                JobState::Done { .. }
                    | JobState::Failed { .. }
                    | JobState::Dismissed
                    | JobState::Expired
            );
            let in_flight = matches!(
                state,
                JobState::Queued
                    | JobState::Submitting
                    | JobState::Polling { .. }
                    | JobState::Downloading
            );
            assert_eq!(state.is_terminal(), terminal);
            assert_eq!(state.is_in_flight(), in_flight);
        }
    }
}
