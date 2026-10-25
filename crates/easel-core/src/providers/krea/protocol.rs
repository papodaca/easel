use std::time::Duration;

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpRequest, HttpResponse, Method};
use crate::job::PollPolicy;
use crate::model_spec::ProviderId;
use crate::providers::{JobId, PollOutcome, Submitted};
use crate::text::sanitize_for_display;
use serde_json::Value;

pub const BILLING_URL: &str = "https://www.krea.ai/docs/developers/api-keys-and-billing";
const STORED_CAP: usize = 64 * 1024;
const SHOWN_CAP: usize = 2 * 1024;

const IN_FLIGHT: &[&str] = &[
    "backlogged",
    "queued",
    "scheduled",
    "processing",
    "sampling",
    "intermediate-complete",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sunset {
    On(String),
    Unknown,
}

#[must_use]
pub fn poll_policy() -> PollPolicy {
    PollPolicy {
        initial: Duration::from_secs(2),
        factor: 1.5,
        cap: Duration::from_secs(5),
        deadline: Duration::from_secs(10 * 60),
    }
}

#[must_use]
pub fn poll_request(submitted: &Submitted) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: submitted.poll_url.to_string(),
        headers: Vec::new(),
        auth: Some(ProviderId::Krea),
        body: Body::Empty,
        max_body_bytes: 2 * 1024 * 1024,
        redact_query: false,
    }
}

#[must_use]
pub fn cancel_request(submitted: &Submitted, last: Option<&PollOutcome>) -> Option<HttpRequest> {
    let status = match last {
        Some(PollOutcome::InFlight { status, .. }) => status.as_str(),
        _ => return None,
    };
    if status != "queued" && status != "processing" {
        return None;
    }
    Some(HttpRequest {
        method: Method::Delete,
        url: format!(
            "{}/jobs/{}",
            ProviderId::Krea.base_url(),
            submitted.job_id.as_str()
        ),
        headers: Vec::new(),
        auth: Some(ProviderId::Krea),
        body: Body::Empty,
        max_body_bytes: 64 * 1024,
        redact_query: false,
    })
}

pub fn parse_submit(response: &HttpResponse) -> Result<Submitted, ProviderError> {
    if response.status != 200 {
        return Err(parse_error(response));
    }
    let value = json_body(&response.body)?;
    let id = value
        .get("job_id")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Protocol("submit response had no job_id".into()))?;
    let job_id = JobId::parse(id)
        .map_err(|_| ProviderError::Protocol("submit response had no job_id".into()))?;
    let poll = format!("{}/jobs/{}", ProviderId::Krea.base_url(), job_id.as_str());
    let poll_url = url::Url::parse(&poll)
        .map_err(|_| ProviderError::Protocol("poll url was not valid".into()))?;
    Ok(Submitted {
        job_id,
        poll_url,
        quote: None,
    })
}

#[must_use]
pub fn parse_poll(response: &HttpResponse) -> PollOutcome {
    if response.status != 200 {
        return PollOutcome::Failed(parse_error(response));
    }
    let Ok(value) = serde_json::from_slice::<Value>(&response.body) else {
        return PollOutcome::Failed(ProviderError::Protocol("poll body was not json".into()));
    };
    let Some(status) = value.get("status").and_then(Value::as_str) else {
        return PollOutcome::Failed(ProviderError::Protocol("poll body had no status".into()));
    };
    if IN_FLIGHT.contains(&status) {
        let progress = value
            .get("progress")
            .and_then(Value::as_f64)
            .map(|value| value.clamp(0.0, 1.0) as f32);
        return PollOutcome::InFlight {
            status: status.to_owned(),
            progress,
        };
    }
    match status {
        "completed" => ready_outcome(&value),
        "failed" => PollOutcome::Failed(failed_protocol(&value)),
        "cancelled" => PollOutcome::Cancelled,
        _ => PollOutcome::Failed(ProviderError::Protocol(format!(
            "unexpected status {}",
            cap_text(status)
        ))),
    }
}

pub fn parse_error(response: &HttpResponse) -> ProviderError {
    match response.status {
        401 | 403 => ProviderError::Auth,
        402 => ProviderError::OutOfCredits {
            hint: Some(BILLING_URL.into()),
        },
        429 => ProviderError::RateLimited { retry_after: None },
        400 => ProviderError::Validation {
            message: cap_text(&error_text(&response.body)),
            field_errors: Vec::<FieldError>::new(),
        },
        410 => gone(&response.body),
        status => ProviderError::Http {
            status,
            body: cap_text(&String::from_utf8_lossy(&response.body)),
        },
    }
}

#[must_use]
pub fn deprecation_headers(response: &HttpResponse) -> Option<Sunset> {
    let sunset = header(response, "sunset");
    let deprecation = header(response, "deprecation");
    match sunset {
        Some(value) => Some(match http_date(&value) {
            Some(date) => Sunset::On(date),
            None => Sunset::Unknown,
        }),
        None if deprecation.is_some() => Some(Sunset::Unknown),
        None => None,
    }
}

#[must_use]
pub fn sunset_toast(sunset: &Sunset) -> String {
    match sunset {
        Sunset::On(date) => format!("Krea sunsets on {date}"),
        Sunset::Unknown => "Krea sunsets on an unknown date".into(),
    }
}

#[must_use]
pub fn shown_error(error: &ProviderError) -> String {
    sanitize_for_display(&error.to_string(), SHOWN_CAP)
}

fn ready_outcome(value: &Value) -> PollOutcome {
    let Some(urls) = value.pointer("/result/urls") else {
        return PollOutcome::Failed(ProviderError::Protocol(
            "completed poll had no result urls".into(),
        ));
    };
    let Value::Array(items) = urls else {
        return PollOutcome::Failed(ProviderError::Protocol("unexpected result shape".into()));
    };
    let Some(url) = items.first().and_then(Value::as_str) else {
        return PollOutcome::Failed(ProviderError::Protocol("unexpected result shape".into()));
    };
    if !ProviderId::Krea.allows_url(url) {
        return PollOutcome::Failed(ProviderError::Protocol("unexpected result host".into()));
    }
    let Ok(url) = url::Url::parse(url) else {
        return PollOutcome::Failed(ProviderError::Protocol("unexpected result host".into()));
    };
    PollOutcome::Ready {
        url,
        settled_cost: None,
        prompt: None,
        duration: None,
    }
}

fn failed_protocol(value: &Value) -> ProviderError {
    let code = value
        .pointer("/error/code")
        .and_then(Value::as_str)
        .unwrap_or("failed");
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    let text = if message.is_empty() {
        code.to_owned()
    } else {
        format!("{code}: {message}")
    };
    ProviderError::Protocol(cap_text(&text))
}

fn gone(body: &[u8]) -> ProviderError {
    let value = serde_json::from_slice::<Value>(body).unwrap_or(Value::Null);
    ProviderError::Gone {
        replacement: value
            .get("replacement")
            .and_then(Value::as_str)
            .map(cap_text),
        sunset: value
            .get("sunset_date")
            .and_then(Value::as_str)
            .map(cap_text),
    }
}

fn error_text(body: &[u8]) -> String {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return "invalid body".into();
    };
    match value.get("error") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Object(_)) => value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("invalid body")
            .to_owned(),
        _ => "invalid body".into(),
    }
}

fn header(response: &HttpResponse, name: &str) -> Option<String> {
    response
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
}

fn http_date(value: &str) -> Option<String> {
    let rest = value.trim().split_once(',')?.1.trim();
    let mut parts = rest.split_whitespace();
    let day = parts.next()?;
    let month = parts.next()?;
    let year = parts.next()?;
    let time = parts.next()?;
    let zone = parts.next()?;
    if parts.next().is_some() || zone != "GMT" {
        return None;
    }
    if day.len() != 2 || !day.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    const MONTHS: &[&str] = &[
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if !MONTHS.contains(&month)
        || year.len() != 4
        || !year.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let bytes = time.as_bytes();
    if bytes.len() != 8 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }
    Some(format!("{day} {month} {year}"))
}

fn json_body(body: &[u8]) -> Result<Value, ProviderError> {
    serde_json::from_slice(body)
        .map_err(|_| ProviderError::Protocol("response body was not json".into()))
}

fn cap_text(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        let next = out.len() + character.len_utf8();
        if next > STORED_CAP {
            break;
        }
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        Sunset, cancel_request, deprecation_headers, parse_error, parse_poll, parse_submit,
        poll_policy, shown_error, sunset_toast,
    };
    use crate::error::ProviderError;
    use crate::http::{HttpHeader, HttpResponse};
    use crate::job::{JobEvent, JobState, PollPolicy, advance};
    use crate::providers::PollOutcome;
    use std::time::Duration;

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn fixture(name: &str) -> String {
        match name {
            "submit" => include_str!("../../../tests/fixtures/krea/submit.json"),
            "completed" => include_str!("../../../tests/fixtures/krea/completed.json"),
            "failed" => include_str!("../../../tests/fixtures/krea/failed.json"),
            "gone" => include_str!("../../../tests/fixtures/krea/gone.json"),
            _ => unreachable!(),
        }
        .to_owned()
    }

    #[test]
    fn submit_builds_the_poll_url_from_the_job_id() {
        let submitted = parse_submit(&response(200, &fixture("submit"))).unwrap();
        assert_eq!(submitted.job_id.as_str(), "job-1");
        assert_eq!(
            submitted.poll_url.as_str(),
            "https://api.krea.ai/jobs/job-1"
        );
        assert!(submitted.quote.is_none());
        let bad = parse_submit(&response(200, r#"{"job_id":"../x","status":"queued"}"#));
        assert!(matches!(bad, Err(ProviderError::Protocol(_))));
    }

    #[test]
    fn poll_maps_each_documented_status() {
        for status in [
            "backlogged",
            "queued",
            "scheduled",
            "processing",
            "sampling",
            "intermediate-complete",
        ] {
            let body = format!(r#"{{"status":"{status}","progress":0.4}}"#);
            match parse_poll(&response(200, &body)) {
                PollOutcome::InFlight {
                    status: got,
                    progress,
                } => {
                    assert_eq!(got, status);
                    assert_eq!(progress, Some(0.4));
                }
                other => panic!("{other:?}"),
            }
        }
        match parse_poll(&response(200, &fixture("completed"))) {
            PollOutcome::Ready {
                url, settled_cost, ..
            } => {
                assert_eq!(url.as_str(), "https://cdn.krea.ai/r.png");
                assert!(settled_cost.is_none());
            }
            other => panic!("{other:?}"),
        }
        match parse_poll(&response(200, &fixture("failed"))) {
            PollOutcome::Failed(ProviderError::Protocol(text)) => {
                assert!(text.contains("boom"), "{text}");
                assert!(text.contains("nope"), "{text}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            parse_poll(&response(200, r#"{"status":"cancelled"}"#)),
            PollOutcome::Cancelled
        );
        let state = advance(
            JobState::Polling {
                attempt: 1,
                progress: None,
            },
            JobEvent::Poll(PollOutcome::Cancelled),
        );
        assert_eq!(state, JobState::Dismissed);
    }

    #[test]
    fn odd_result_shapes_and_hosts_fail_clearly() {
        let mapped = parse_poll(&response(
            200,
            r#"{"status":"completed","result":{"urls":{"full":"https://cdn.krea.ai/r.png"}}}"#,
        ));
        assert!(matches!(
            mapped,
            PollOutcome::Failed(ProviderError::Protocol(text)) if text.contains("unexpected result shape")
        ));
        let evil = parse_poll(&response(
            200,
            r#"{"status":"completed","result":{"urls":["https://evil.example/r.png"]}}"#,
        ));
        assert!(matches!(
            evil,
            PollOutcome::Failed(ProviderError::Protocol(text)) if text.contains("unexpected result host")
        ));
    }

    #[test]
    fn http_errors_use_the_krea_mapping() {
        assert!(matches!(
            parse_error(&response(401, "no")),
            ProviderError::Auth
        ));
        match parse_error(&response(402, "no")) {
            ProviderError::OutOfCredits { hint: Some(hint) } => {
                assert!(hint.contains("krea.ai"), "{hint}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            parse_error(&response(429, "no")),
            ProviderError::RateLimited { retry_after: None }
        );
        match parse_error(&response(400, r#"{"error":"bad prompt"}"#)) {
            ProviderError::Validation { message, .. } => assert_eq!(message, "bad prompt"),
            other => panic!("{other:?}"),
        }
        match parse_error(&response(410, &fixture("gone"))) {
            ProviderError::Gone {
                replacement: Some(name),
                sunset: Some(date),
            } => {
                assert_eq!(name, "krea-2/large");
                assert_eq!(date, "2026-04-27");
                assert!(
                    parse_error(&response(410, &fixture("gone")))
                        .to_string()
                        .contains("krea-2/large")
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_long_replacement_is_stored_under_64kb_and_shown_under_2kb() {
        let name = "n".repeat(10 * 1024);
        let body = format!(
            r#"{{"error":"model_unavailable","replacement":"{name}","sunset_date":"2026-04-27"}}"#
        );
        let error = parse_error(&response(410, &body));
        let ProviderError::Gone {
            replacement: Some(stored),
            ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert!(stored.len() <= 64 * 1024, "{}", stored.len());
        assert_eq!(stored.len(), 10 * 1024);
        let shown = shown_error(&error);
        assert!(shown.len() <= 2 * 1024, "{}", shown.len());
        assert!(shown.contains('n'));
    }

    #[test]
    fn sunset_headers_parse_or_stay_unknown() {
        assert!(deprecation_headers(&response(200, "{}")).is_none());
        let dated = HttpResponse {
            status: 200,
            headers: vec![
                HttpHeader {
                    name: "Deprecation".into(),
                    value: "true".into(),
                    sensitive: false,
                },
                HttpHeader {
                    name: "Sunset".into(),
                    value: "Sun, 06 Nov 1994 08:49:37 GMT".into(),
                    sensitive: false,
                },
            ],
            body: Vec::new(),
        };
        assert_eq!(
            deprecation_headers(&dated),
            Some(Sunset::On("06 Nov 1994".into()))
        );
        let raw = "not-a-date";
        let unknown = HttpResponse {
            status: 200,
            headers: vec![HttpHeader {
                name: "Sunset".into(),
                value: raw.into(),
                sensitive: false,
            }],
            body: Vec::new(),
        };
        assert_eq!(deprecation_headers(&unknown), Some(Sunset::Unknown));
        let toast = sunset_toast(&Sunset::Unknown);
        assert!(toast.contains("unknown date"));
        assert!(!toast.contains(raw));
    }

    #[test]
    fn cancel_is_only_for_queued_or_processing() {
        let submitted = parse_submit(&response(200, &fixture("submit"))).unwrap();
        let processing = PollOutcome::InFlight {
            status: "processing".into(),
            progress: Some(0.2),
        };
        let cancel = cancel_request(&submitted, Some(&processing)).unwrap();
        assert_eq!(cancel.method, crate::http::Method::Delete);
        assert_eq!(cancel.url, "https://api.krea.ai/jobs/job-1");
        assert!(cancel_request(&submitted, None).is_none());
        let ready = PollOutcome::Ready {
            url: "https://cdn.krea.ai/r.png".parse().unwrap(),
            settled_cost: None,
            prompt: None,
            duration: None,
        };
        assert!(cancel_request(&submitted, Some(&ready)).is_none());
    }

    #[test]
    fn krea_poll_waits_longer_than_bfl() {
        let policy = poll_policy();
        assert_eq!(policy.initial, Duration::from_secs(2));
        assert_eq!(policy.next_delay(1), Duration::from_secs(3));
        assert_eq!(policy.next_delay(2).as_millis(), 4500);
        assert_eq!(policy.next_delay(3), Duration::from_secs(5));
        assert_eq!(policy.deadline, Duration::from_secs(10 * 60));
        assert_eq!(
            PollPolicy::default_bfl().deadline,
            Duration::from_secs(5 * 60)
        );
    }
}
