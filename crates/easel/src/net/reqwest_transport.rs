use std::sync::{Arc, Once};
use std::time::Duration;

use anyhow::anyhow;
use easel_core::{
    http::{Body, HttpHeader, HttpRequest, HttpResponse, Method, Part},
    model_spec::AuthScheme,
};
use gpui::{App, Task};

use super::{HttpTransport, KeyResolver, TransportError};

const ONE_MB: u64 = 1_000_000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ReqwestTransport {
    client: reqwest::Client,
    keys: Arc<dyn KeyResolver>,
    #[cfg(test)]
    total_override: Option<Duration>,
}

impl ReqwestTransport {
    pub fn new(keys: impl KeyResolver + 'static) -> Self {
        Self {
            client: build_client(),
            keys: Arc::new(keys),
            #[cfg(test)]
            total_override: None,
        }
    }

    #[cfg(test)]
    pub fn with_total_timeout(keys: impl KeyResolver + 'static, timeout: Duration) -> Self {
        Self {
            client: build_client(),
            keys: Arc::new(keys),
            total_override: Some(timeout),
        }
    }

    fn budget(&self, request: &HttpRequest) -> Duration {
        #[cfg(test)]
        if let Some(timeout) = self.total_override {
            return timeout;
        }
        total_timeout(request)
    }
}

pub fn total_timeout(request: &HttpRequest) -> Duration {
    let long = matches!(request.body, Body::Multipart { .. })
        || encoded_len(&request.body) > ONE_MB
        || request.max_body_bytes > ONE_MB;
    Duration::from_secs(if long { 180 } else { 60 })
}

impl HttpTransport for ReqwestTransport {
    fn request(
        &self,
        request: HttpRequest,
        cx: &mut App,
    ) -> Task<Result<HttpResponse, TransportError>> {
        let method = method_label(request.method).to_owned();
        let display_url = display_url(&request);
        let client = self.client.clone();
        let keys = Arc::clone(&self.keys);
        let budget = self.budget(&request);
        let joined = gpui_tokio::Tokio::spawn_result(cx, async move {
            execute(&client, request, keys.as_ref(), budget)
                .await
                .map_err(|err| anyhow!(TransportFailure(err)))
        });
        cx.spawn(async move |_| match joined.await {
            Ok(response) => Ok(response),
            Err(err) => match err.downcast::<TransportFailure>() {
                Ok(failure) => Err(failure.0),
                Err(_) => Err(TransportError::Network {
                    method,
                    url: display_url,
                }),
            },
        })
    }
}

#[derive(Debug)]
struct TransportFailure(TransportError);

impl std::fmt::Display for TransportFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl std::error::Error for TransportFailure {}

async fn execute(
    client: &reqwest::Client,
    mut request: HttpRequest,
    keys: &dyn KeyResolver,
    budget: Duration,
) -> Result<HttpResponse, TransportError> {
    let method = method_label(request.method).to_owned();
    let url = request.url.clone();
    let display_url = display_url(&request);
    ensure_url(&url)?;
    apply_auth(&mut request, keys)?;
    log_request(&request, &display_url);

    let reqwest_method = match request.method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Delete => reqwest::Method::DELETE,
    };
    let mut builder = client.request(reqwest_method, &url).timeout(budget);
    for header in &request.headers {
        builder = builder.header(
            header.name.as_str(),
            header_value(header).map_err(|_| TransportError::Network {
                method: method.clone(),
                url: display_url.clone(),
            })?,
        );
    }
    builder = match &request.body {
        Body::Empty => builder,
        Body::Multipart { parts } => {
            builder.multipart(multipart_form(parts).map_err(|_| TransportError::Network {
                method: method.clone(),
                url: display_url.clone(),
            })?)
        }
        Body::Json(value) => {
            let bytes = serde_json::to_vec(value).map_err(|_| TransportError::Network {
                method: method.clone(),
                url: display_url.clone(),
            })?;
            builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(bytes)
        }
        Body::Bytes(bytes, content_type) => builder
            .header(reqwest::header::CONTENT_TYPE, content_type.as_str())
            .body(bytes.clone()),
    };

    let response = builder
        .send()
        .await
        .map_err(|err| map_reqwest(err, &method, &display_url))?;
    let status = response.status().as_u16();
    log::debug!("{method} {display_url} status {status}");
    let headers = response_headers(&response);
    let body = read_body(response, request.max_body_bytes, &method, &display_url).await?;
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn apply_auth(request: &mut HttpRequest, keys: &dyn KeyResolver) -> Result<(), TransportError> {
    let Some(provider) = request.auth else {
        return Ok(());
    };
    let Some(key) = keys.key_for(provider).filter(|key| !key.is_empty()) else {
        return Err(TransportError::NoCredential(provider));
    };
    let (name, value) = match provider.auth_scheme() {
        AuthScheme::HeaderKey { header } => (header, key.to_string()),
        AuthScheme::Bearer => (
            "authorization".to_owned(),
            format!("Bearer {}", key.as_str()),
        ),
    };
    request
        .headers
        .retain(|header| !header.name.eq_ignore_ascii_case(&name));
    request.headers.push(HttpHeader {
        name,
        value,
        sensitive: true,
    });
    Ok(())
}

fn multipart_form(parts: &[Part]) -> Result<reqwest::multipart::Form, ()> {
    let mut form = reqwest::multipart::Form::new();
    for part in parts {
        let mut encoded = reqwest::multipart::Part::bytes(part.bytes.clone());
        if let Some(filename) = &part.filename {
            encoded = encoded.file_name(filename.clone());
        }
        encoded = encoded.mime_str(&part.content_type).map_err(|_| ())?;
        form = form.part(part.name.clone(), encoded);
    }
    Ok(form)
}

fn header_value(header: &HttpHeader) -> Result<reqwest::header::HeaderValue, ()> {
    let mut value = reqwest::header::HeaderValue::from_str(&header.value).map_err(|_| ())?;
    if header.sensitive || credential_name(&header.name) {
        value.set_sensitive(true);
    }
    Ok(value)
}

fn response_headers(response: &reqwest::Response) -> Vec<HttpHeader> {
    response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            let value = value.to_str().ok()?.to_owned();
            let name = name.as_str().to_owned();
            let sensitive = credential_name(&name);
            Some(HttpHeader {
                name,
                value,
                sensitive,
            })
        })
        .collect()
}

async fn read_body(
    mut response: reqwest::Response,
    max_body_bytes: u64,
    method: &str,
    url: &str,
) -> Result<Vec<u8>, TransportError> {
    if response
        .content_length()
        .is_some_and(|len| len > max_body_bytes)
    {
        return Err(TransportError::TooLarge {
            method: method.to_owned(),
            url: url.to_owned(),
        });
    }
    let mut body = bytes::BytesMut::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| map_reqwest(err, method, url))?
    {
        let chunk: bytes::Bytes = chunk;
        let next = body.len() as u64 + chunk.len() as u64;
        if next > max_body_bytes {
            return Err(TransportError::TooLarge {
                method: method.to_owned(),
                url: url.to_owned(),
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.to_vec())
}

fn map_reqwest(err: reqwest::Error, method: &str, url: &str) -> TransportError {
    if err.is_timeout() {
        TransportError::Timeout {
            method: method.to_owned(),
            url: url.to_owned(),
        }
    } else {
        TransportError::Network {
            method: method.to_owned(),
            url: url.to_owned(),
        }
    }
}

fn log_request(request: &HttpRequest, display_url: &str) {
    let method = method_label(request.method);
    let url = display_url;
    log::debug!("{method} {url}");
    for header in &request.headers {
        if header.sensitive || credential_name(&header.name) {
            log::debug!("{method} {url} header {}: <redacted>", header.name);
        } else {
            log::debug!("{method} {url} header {}: {}", header.name, header.value);
        }
    }
    if log::log_enabled!(log::Level::Trace) {
        let bytes = body_bytes(&request.body);
        let end = bytes.len().min(1024);
        let text = String::from_utf8_lossy(&bytes[..end]);
        log::trace!("{method} {url} body {text}");
    }
}

/// The URL as logs and transport errors report it: the query is `<redacted>`
/// for requests that asked for it, and the scheme, host, and path stay.
fn display_url(request: &HttpRequest) -> String {
    if !request.redact_query {
        return request.url.clone();
    }
    match request.url.split_once('?') {
        Some((base, _)) => format!("{base}?<redacted>"),
        None => request.url.clone(),
    }
}

fn body_bytes(body: &Body) -> Vec<u8> {
    match body {
        Body::Empty | Body::Multipart { .. } => Vec::new(),
        Body::Json(value) => serde_json::to_vec(value).unwrap_or_default(),
        Body::Bytes(bytes, _) => bytes.clone(),
    }
}

fn encoded_len(body: &Body) -> u64 {
    match body {
        Body::Empty => 0,
        Body::Json(value) => serde_json::to_vec(value).map_or(u64::MAX, |bytes| bytes.len() as u64),
        Body::Bytes(bytes, _) => bytes.len() as u64,
        Body::Multipart { parts } => parts.iter().map(|part| part.bytes.len() as u64).sum(),
    }
}

fn ensure_url(url: &str) -> Result<(), TransportError> {
    if url.starts_with("https://") {
        return Ok(());
    }
    #[cfg(test)]
    if loopback_http(url) {
        return Ok(());
    }
    Err(TransportError::InsecureUrl {
        url: url.to_owned(),
    })
}

#[cfg(test)]
fn loopback_http(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once(':').map(|(host, _)| host).unwrap_or(host);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

fn credential_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "x-key"
            | "x-goog-api-key"
    )
}

fn method_label(method: Method) -> &'static str {
    match method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Delete => "DELETE",
    }
}

fn build_client() -> reqwest::Client {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("http client")
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, Once};
    use std::time::Duration;

    use easel_core::{
        http::{Body, HttpHeader, HttpRequest, HttpResponse, Method},
        model_spec::ProviderId,
    };
    use gpui::{TestAppContext, prelude::*};
    use mockito::{Server, ServerOpts};
    use serde_json::json;

    use super::total_timeout;
    use crate::net::{FakeTransport, HttpTransport, MapKeys, ReqwestTransport, TransportError};

    struct LineLog;

    static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static LOG_ONCE: Once = Once::new();

    impl log::Log for LineLog {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.target().starts_with("easel") && metadata.level() <= log::Level::Debug
        }

        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                LINES
                    .lock()
                    .expect("lines")
                    .push(format!("{}", record.args()));
            }
        }

        fn flush(&self) {}
    }

    fn install_capture() {
        LOG_ONCE.call_once(|| {
            log::set_logger(&LineLog).expect("test logger");
            log::set_max_level(log::LevelFilter::Debug);
        });
    }

    fn lines_for(url: &str) -> String {
        LINES
            .lock()
            .expect("lines")
            .iter()
            .filter(|line| line.contains(url))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn boot(cx: &mut TestAppContext, keys: MapKeys) -> ReqwestTransport {
        cx.update(|cx| {
            gpui_tokio::init(cx);
            ReqwestTransport::new(keys)
        })
    }

    fn boot_timeout(cx: &mut TestAppContext, timeout: Duration) -> ReqwestTransport {
        cx.update(|cx| {
            gpui_tokio::init(cx);
            ReqwestTransport::with_total_timeout(MapKeys::default(), timeout)
        })
    }

    fn request(method: Method, url: impl Into<String>, body: Body) -> HttpRequest {
        HttpRequest {
            method,
            url: url.into(),
            headers: Vec::new(),
            auth: None,
            body,
            max_body_bytes: 1024,
            redact_query: false,
        }
    }

    async fn dispatch(
        cx: &mut TestAppContext,
        transport: &ReqwestTransport,
        request: HttpRequest,
    ) -> Result<HttpResponse, TransportError> {
        cx.executor().allow_parking();
        let task = cx.update(|cx| transport.request(request, cx));
        task.await
    }

    fn server() -> Server {
        Server::new_with_opts(ServerOpts::default())
    }

    #[gpui::test]
    async fn get_returns_status_and_body(cx: &mut TestAppContext) {
        let mut server = server();
        let mock = server
            .mock("GET", "/ok")
            .with_status(200)
            .with_body("{\"a\":1}")
            .create_async()
            .await;
        let transport = boot(cx, MapKeys::default());
        let url = format!("{}/ok", server.url());
        let response = dispatch(cx, &transport, request(Method::Get, url, Body::Empty))
            .await
            .expect("response");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"{\"a\":1}");
        mock.assert_async().await;
    }

    #[gpui::test]
    async fn post_json_sends_content_type_and_body(cx: &mut TestAppContext) {
        let mut server = server();
        let mock = server
            .mock("POST", "/echo")
            .match_header("content-type", "application/json")
            .match_body(r#"{"a":1}"#)
            .with_status(201)
            .with_body("saved")
            .create_async()
            .await;
        let transport = boot(cx, MapKeys::default());
        let url = format!("{}/echo", server.url());
        let response = dispatch(
            cx,
            &transport,
            request(Method::Post, url, Body::Json(json!({"a": 1}))),
        )
        .await
        .expect("response");
        assert_eq!(response.status, 201);
        mock.assert_async().await;
    }

    #[gpui::test]
    async fn bfl_key_is_sent_as_x_key(cx: &mut TestAppContext) {
        let mut server = server();
        let mock = server
            .mock("GET", "/keyed")
            .match_header("x-key", "bfl-key-9f3a")
            .with_status(204)
            .create_async()
            .await;
        let transport = boot(
            cx,
            MapKeys::default().insert(ProviderId::Bfl, "bfl-key-9f3a"),
        );
        let url = format!("{}/keyed", server.url());
        let mut http = request(Method::Get, &url, Body::Empty);
        http.auth = Some(ProviderId::Bfl);
        let response = dispatch(cx, &transport, http).await.expect("response");
        assert_eq!(response.status, 204);
        mock.assert_async().await;
    }

    #[gpui::test]
    async fn krea_key_is_sent_as_bearer(cx: &mut TestAppContext) {
        let mut server = server();
        let mock = server
            .mock("GET", "/bearer")
            .match_header("authorization", "Bearer krea-key-9f3a")
            .with_status(204)
            .create_async()
            .await;
        let transport = boot(
            cx,
            MapKeys::default().insert(ProviderId::Krea, "krea-key-9f3a"),
        );
        let url = format!("{}/bearer", server.url());
        let mut http = request(Method::Get, url, Body::Empty);
        http.auth = Some(ProviderId::Krea);
        let response = dispatch(cx, &transport, http).await.expect("response");
        assert_eq!(response.status, 204);
        mock.assert_async().await;
    }

    #[gpui::test]
    async fn missing_key_makes_no_http_call(cx: &mut TestAppContext) {
        let mut server = server();
        let mock = server.mock("GET", "/locked").expect(0).create_async().await;
        let transport = boot(cx, MapKeys::default());
        let url = format!("{}/locked", server.url());
        let mut http = request(Method::Get, url, Body::Empty);
        http.auth = Some(ProviderId::Bfl);
        let err = dispatch(cx, &transport, http)
            .await
            .expect_err("missing key");
        assert!(matches!(err, TransportError::NoCredential(ProviderId::Bfl)));
        mock.assert_async().await;
    }

    #[gpui::test]
    async fn sensitive_values_are_redacted_in_logs_and_errors(cx: &mut TestAppContext) {
        install_capture();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        drop(listener);
        let url = format!("http://{address}/secret");
        let mut http = request(
            Method::Post,
            &url,
            Body::Bytes(
                b"sekrit-body-zz".to_vec(),
                "application/octet-stream".into(),
            ),
        );
        http.headers.push(HttpHeader {
            name: "x-key".into(),
            value: "sekrit-key-zz".into(),
            sensitive: true,
        });
        http.auth = Some(ProviderId::Bfl);
        let request_debug = format!("{http:?}");
        assert!(request_debug.contains("<redacted>"));
        assert!(!request_debug.contains("sekrit-key-zz"));
        assert!(!request_debug.contains("sekrit-body-zz"));

        let transport = boot(
            cx,
            MapKeys::default().insert(ProviderId::Bfl, "sekrit-key-zz"),
        );
        let err = dispatch(cx, &transport, http).await.expect_err("refused");
        let error_debug = format!("{err:?}");
        assert!(!error_debug.contains("sekrit-key-zz"), "{error_debug}");
        assert!(!error_debug.contains("sekrit-body-zz"), "{error_debug}");
        let logged = lines_for(&url);
        assert!(logged.contains("<redacted>"), "{logged}");
        assert!(!logged.contains("sekrit-key-zz"), "{logged}");
        assert!(!logged.contains("sekrit-body-zz"), "{logged}");
    }

    #[gpui::test]
    async fn not_found_is_a_response_and_refused_is_network(cx: &mut TestAppContext) {
        let mut server = server();
        let mock = server
            .mock("GET", "/missing")
            .with_status(404)
            .with_body("gone")
            .create_async()
            .await;
        let transport = boot(cx, MapKeys::default());
        let url = format!("{}/missing", server.url());
        let response = dispatch(cx, &transport, request(Method::Get, url, Body::Empty))
            .await
            .expect("404");
        assert_eq!(response.status, 404);
        assert_eq!(response.body, b"gone");
        mock.assert_async().await;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        drop(listener);
        let err = dispatch(
            cx,
            &transport,
            request(Method::Get, format!("http://{address}/nope"), Body::Empty),
        )
        .await
        .expect_err("refused");
        assert!(matches!(err, TransportError::Network { .. }), "{err:?}");
    }

    #[gpui::test]
    async fn credentialed_redirect_is_not_followed(cx: &mut TestAppContext) {
        let mut first = server();
        let mut second = server();
        let land = second.mock("GET", "/land").expect(0).create_async().await;
        let location = format!("{}/land", second.url());
        let jump = first
            .mock("GET", "/jump")
            .with_status(302)
            .with_header("location", &location)
            .create_async()
            .await;
        let transport = boot(
            cx,
            MapKeys::default().insert(ProviderId::Bfl, "bfl-key-9f3a"),
        );
        let url = format!("{}/jump", first.url());
        let mut http = request(Method::Get, url, Body::Empty);
        http.auth = Some(ProviderId::Bfl);
        let response = dispatch(cx, &transport, http).await.expect("redirect");
        assert_eq!(response.status, 302);
        jump.assert_async().await;
        land.assert_async().await;
    }

    #[gpui::test]
    async fn oversized_content_length_and_chunked_body(cx: &mut TestAppContext) {
        let mut server = server();
        let counted = server
            .mock("GET", "/counted")
            .with_body("0123456789")
            .create_async()
            .await;
        let chunked = server
            .mock("GET", "/chunked")
            .with_chunked_body(|writer| writer.write_all(b"0123456789"))
            .create_async()
            .await;
        let transport = boot(cx, MapKeys::default());
        let mut counted_request = request(
            Method::Get,
            format!("{}/counted", server.url()),
            Body::Empty,
        );
        counted_request.max_body_bytes = 4;
        let err = dispatch(cx, &transport, counted_request)
            .await
            .expect_err("length");
        assert!(matches!(err, TransportError::TooLarge { .. }), "{err:?}");

        let mut chunked_request = request(
            Method::Get,
            format!("{}/chunked", server.url()),
            Body::Empty,
        );
        chunked_request.max_body_bytes = 4;
        let err = dispatch(cx, &transport, chunked_request)
            .await
            .expect_err("chunked");
        assert!(matches!(err, TransportError::TooLarge { .. }), "{err:?}");
        counted.assert_async().await;
        chunked.assert_async().await;
    }

    #[gpui::test]
    async fn insecure_public_http_is_rejected_and_loopback_works(cx: &mut TestAppContext) {
        let transport = boot_timeout(cx, Duration::from_millis(200));
        let err = dispatch(
            cx,
            &transport,
            request(Method::Get, "http://api.bfl.ai/v1/get_result", Body::Empty),
        )
        .await
        .expect_err("insecure");
        assert!(matches!(err, TransportError::InsecureUrl { .. }), "{err:?}");

        let mut server = server();
        let mock = server
            .mock("GET", "/loop")
            .with_status(200)
            .with_body("ok")
            .create_async()
            .await;
        let url = format!("{}/loop", server.url());
        assert!(url.starts_with("http://127.0.0.1:"));
        let response = dispatch(cx, &transport, request(Method::Get, url, Body::Empty))
            .await
            .expect("loopback");
        assert_eq!(response.status, 200);
        mock.assert_async().await;
    }

    #[gpui::test]
    async fn delayed_response_times_out(cx: &mut TestAppContext) {
        let mut server = server();
        let mock = server
            .mock("GET", "/slow")
            .with_chunked_body(|writer| {
                std::thread::sleep(Duration::from_millis(800));
                writer.write_all(b"late")
            })
            .create_async()
            .await;
        let transport = boot_timeout(cx, Duration::from_millis(150));
        let url = format!("{}/slow", server.url());
        let err = dispatch(cx, &transport, request(Method::Get, url, Body::Empty))
            .await
            .expect_err("timeout");
        assert!(matches!(err, TransportError::Timeout { .. }), "{err:?}");
        mock.assert_async().await;
    }

    #[test]
    fn timeout_budget_follows_body_and_download_size() {
        let small = request(
            Method::Get,
            "https://api.bfl.ai/v1/x",
            Body::Json(serde_json::Value::String("a".repeat(1024))),
        );
        assert_eq!(total_timeout(&small), Duration::from_secs(60));

        let large = request(
            Method::Post,
            "https://api.bfl.ai/v1/x",
            Body::Json(serde_json::Value::String("a".repeat(2_000_000))),
        );
        assert_eq!(total_timeout(&large), Duration::from_secs(180));

        let mut download = request(Method::Get, "https://api.bfl.ai/v1/x", Body::Empty);
        download.max_body_bytes = 100_000_000;
        assert_eq!(total_timeout(&download), Duration::from_secs(180));

        let upload = request(
            Method::Post,
            "https://api.bfl.ai/v1/x",
            Body::Multipart { parts: Vec::new() },
        );
        assert_eq!(total_timeout(&upload), Duration::from_secs(180));
    }

    #[gpui::test]
    async fn debug_log_omits_a_megabyte_body(cx: &mut TestAppContext) {
        install_capture();
        let mut server = server();
        let mock = server
            .mock("POST", "/blob")
            .with_status(204)
            .create_async()
            .await;
        let transport = boot(cx, MapKeys::default());
        let url = format!("{}/blob", server.url());
        let marker = b"BODYMARKER".repeat(100_000);
        assert_eq!(marker.len(), 1_000_000);
        let response = dispatch(
            cx,
            &transport,
            request(
                Method::Post,
                &url,
                Body::Bytes(marker, "application/octet-stream".into()),
            ),
        )
        .await
        .expect("posted");
        assert_eq!(response.status, 204);
        let logged = lines_for(&url);
        assert!(!logged.contains("BODYMARKER"), "{logged}");
        mock.assert_async().await;
    }

    #[gpui::test]
    async fn dropping_the_task_does_not_deliver_a_late_update(cx: &mut TestAppContext) {
        let mut server = server();
        let mock = server
            .mock("GET", "/slow")
            .expect_at_most(1)
            .with_chunked_body(|writer| {
                std::thread::sleep(Duration::from_millis(800));
                writer.write_all(b"late")
            })
            .create_async()
            .await;
        let transport = boot_timeout(cx, Duration::from_secs(5));
        let url = format!("{}/slow", server.url());
        let (sender, receiver) = std::sync::mpsc::channel();
        cx.executor().allow_parking();
        let task = cx.update(|cx| {
            let request_task = transport.request(request(Method::Get, url, Body::Empty), cx);
            cx.spawn(async move |_| {
                let result = request_task.await;
                let _ = sender.send(result.is_ok());
            })
        });
        cx.run_until_parked();
        drop(task);
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            receiver.try_recv().is_err(),
            "dropped task delivered a result"
        );
        std::thread::sleep(Duration::from_millis(900));
        mock.assert_async().await;
    }

    #[test]
    fn bearer_header_debug_redacts_the_key() {
        use easel_core::model_spec::AuthScheme;

        let mut http = request(Method::Get, "https://api.krea.ai/jobs/x", Body::Empty);
        http.auth = Some(ProviderId::Krea);
        assert!(matches!(ProviderId::Krea.auth_scheme(), AuthScheme::Bearer));
        assert!(matches!(
            ProviderId::Bfl.auth_scheme(),
            AuthScheme::HeaderKey { ref header } if header == "x-key"
        ));
        super::apply_auth(
            &mut http,
            &MapKeys::default().insert(ProviderId::Krea, "krea-key-9f3a"),
        )
        .expect("auth");
        let debug = format!("{http:?}");
        assert!(debug.contains("<redacted>"), "{debug}");
        assert!(!debug.contains("krea-key-9f3a"), "{debug}");
    }

    #[test]
    fn google_api_key_is_redacted_when_the_header_is_not_marked_sensitive() {
        install_capture();
        let key = "gemini-secret-key-zz";
        let url = "https://generativelanguage.googleapis.com/v1beta/models";
        let mut http = request(
            Method::Get,
            url,
            Body::Json(json!({"model": "gemini-nano-banana-2.1"})),
        );
        assert!(!http.url.contains("key="));
        http.headers.push(HttpHeader {
            name: "x-goog-api-key".into(),
            value: key.into(),
            sensitive: false,
        });
        super::log_request(&http, super::display_url(&http).as_str());
        let logged = lines_for(url);
        assert!(logged.contains("<redacted>"), "{logged}");
        assert!(!logged.contains(key), "{logged}");
        assert!(!logged.contains("key="), "{logged}");
    }

    #[gpui::test]
    async fn a_redacted_picture_get_sends_the_query_and_logs_it_masked(cx: &mut TestAppContext) {
        install_capture();
        let mut server = server();
        let mock = server
            .mock("GET", "/a.png?Expires=170&Signature=abc")
            .with_status(200)
            .create_async()
            .await;
        let transport = boot(cx, MapKeys::default());
        let url = format!("{}/a.png?Expires=170&Signature=abc", server.url());
        let mut http = request(Method::Get, &url, Body::Empty);
        http.redact_query = true;
        let response = dispatch(cx, &transport, http).await.expect("picture");
        assert_eq!(response.status, 200);
        mock.assert_async().await;
        let logged = lines_for(&format!("{}/a.png", server.url()));
        assert!(logged.contains("<redacted>"), "{logged}");
        assert!(!logged.contains("Expires=170"), "{logged}");
        assert!(!logged.contains("Signature=abc"), "{logged}");
    }

    #[gpui::test]
    async fn a_redacted_transport_error_keeps_the_path_and_masks_the_query(
        cx: &mut TestAppContext,
    ) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        drop(listener);
        let transport = boot_timeout(cx, Duration::from_secs(1));
        let mut http = request(
            Method::Get,
            format!("http://{address}/a.png?Expires=secret&Signature=top"),
            Body::Empty,
        );
        http.redact_query = true;
        let err = dispatch(cx, &transport, http).await.expect_err("refused");
        let text = err.to_string();
        assert!(text.contains("<redacted>"), "{text}");
        assert!(text.contains("/a.png"), "{text}");
        assert!(!text.contains("Expires=secret"), "{text}");
        assert!(!text.contains("Signature=top"), "{text}");
    }

    #[gpui::test]
    async fn multipart_sends_the_file_part(cx: &mut TestAppContext) {
        use easel_core::http::Part;

        let mut server = server();
        let mock = server
            .mock("POST", "/parts")
            .match_header(
                "content-type",
                mockito::Matcher::Regex(r"^multipart/form-data".into()),
            )
            .match_body(mockito::Matcher::Regex(r#"filename="image.png""#.into()))
            .with_status(201)
            .with_body(r#"{"image_url":"https://api.krea.ai/a"}"#)
            .create_async()
            .await;
        let transport = boot(cx, MapKeys::default());
        let url = format!("{}/parts", server.url());
        let response = dispatch(
            cx,
            &transport,
            request(
                Method::Post,
                url,
                Body::Multipart {
                    parts: vec![Part {
                        name: "file".into(),
                        filename: Some("image.png".into()),
                        content_type: "image/png".into(),
                        bytes: b"png-bytes".to_vec(),
                    }],
                },
            ),
        )
        .await
        .expect("multipart");
        assert_eq!(response.status, 201);
        mock.assert_async().await;
    }

    #[gpui::test]
    async fn view_updates_when_the_fake_transport_responds(cx: &mut TestAppContext) {
        struct Probe {
            status: Option<u16>,
        }

        let transport = FakeTransport::scripted(vec![Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: b"ok".to_vec(),
        })]);
        let probe = cx.new(|_| Probe { status: None });
        probe.update(cx, |_probe, cx| {
            let mut http = request(Method::Get, "https://api.bfl.ai/v1/x", Body::Empty);
            http.auth = Some(ProviderId::Bfl);
            let task = transport.request(http, cx);
            cx.spawn(async move |probe, cx| {
                let response = task.await.expect("response");
                probe
                    .update(cx, |probe, cx| {
                        probe.status = Some(response.status);
                        cx.notify();
                    })
                    .ok();
            })
            .detach();
        });
        cx.run_until_parked();
        assert_eq!(probe.read_with(cx, |probe, _| probe.status), Some(200));
        let seen = transport.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].auth, Some(ProviderId::Bfl));
    }
}
