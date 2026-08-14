// Standard
use std::sync::Arc;

// Third Party
use alog::{MessageLevel, alog_channel, use_channel};
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use futures_util::{Stream, StreamExt, stream};
use tokio::sync::oneshot;

// Local
use crate::proxy::usage::{self, UsageStats, UsageTracker};
use crate::registry::Secret;

use_channel!("PRXY");

/*-- public --*/

/// A running reverse-proxy for one upstream model endpoint. Forwards every
/// request to the real upstream, injecting the real credentials and
/// recording token usage under `label` as responses pass through.
pub struct ProxyServer {
    pub local_base_url: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl ProxyServer {
    /// Bind an ephemeral localhost port and start proxying to `base_url`.
    /// Usage observed in responses is recorded into `tracker` under `label`
    /// (e.g. the capability id).
    ///
    /// Synchronous: binds via a plain OS `std::net::TcpListener` call and
    /// hands it to `tokio::spawn`, which only needs an *ambient* Tokio
    /// runtime, not an `.await` -- this lets callers start a proxy from
    /// inside sync code (e.g. `ConfigConstructable::new`) as long as a
    /// runtime is already running somewhere up the call stack.
    pub fn start(
        base_url: String,
        api_key: Option<Secret>,
        verify_ssl: bool,
        tracker: Arc<UsageTracker>,
        label: String,
    ) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(!verify_ssl)
            .build()?;
        let state = Arc::new(UpstreamState {
            client,
            base_url,
            api_key,
            tracker,
            label,
        });

        let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        std_listener.set_nonblocking(true)?;
        let local_addr = std_listener.local_addr()?;
        let local_base_url = format!("http://{local_addr}");
        let listener = tokio::net::TcpListener::from_std(std_listener)?;

        let app = Router::new().fallback(any(proxy_handler)).with_state(state);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let join_handle = tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            if let Err(e) = server.await {
                alog_channel!(
                    MessageLevel::Warning,
                    "usage-tracking proxy server error: {e}"
                );
            }
        });

        Ok(Self {
            local_base_url,
            shutdown_tx: Some(shutdown_tx),
            join_handle,
        })
    }

    /// Signal the server to stop accepting new connections and wait for it
    /// to finish draining in-flight ones.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = self.join_handle.await;
    }
}

/*-- private --*/

struct UpstreamState {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<Secret>,
    tracker: Arc<UsageTracker>,
    label: String,
}

/// Request headers that must not be blindly forwarded upstream: hop-by-hop
/// headers are connection-specific, and the auth headers are replaced with
/// the real credentials so the launched agent never needs to see (or send)
/// them.
fn is_forbidden_request_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "authorization"
            | "x-api-key"
            | "content-length"
            | "connection"
            | "keep-alive"
            | "transfer-encoding"
            | "te"
            | "trailer"
            | "upgrade"
            | "proxy-authenticate"
            | "proxy-authorization"
    )
}

/// Response headers dropped when relaying upstream's response back to the
/// client: framing is re-derived from the `Body` we construct, not copied.
fn is_forbidden_response_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection" | "keep-alive" | "transfer-encoding" | "content-length"
    )
}

async fn proxy_handler(
    State(state): State<Arc<UpstreamState>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Response {
    match forward(&state, method, uri, headers, body).await {
        Ok(response) => response,
        Err(e) => {
            alog_channel!(
                MessageLevel::Warning,
                "usage-tracking proxy forward failed: {e}"
            );
            (StatusCode::BAD_GATEWAY, format!("proxy error: {e}")).into_response()
        }
    }
}

async fn forward(
    state: &UpstreamState,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> anyhow::Result<Response> {
    let path_and_query = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    let url = format!("{}{}", state.base_url.trim_end_matches('/'), path_and_query);
    let body_bytes = axum::body::to_bytes(body, usize::MAX).await?;

    let outbound_method = reqwest::Method::from_bytes(method.as_str().as_bytes())?;
    let mut outbound = state.client.request(outbound_method, &url);
    for (name, value) in headers.iter() {
        if is_forbidden_request_header(name.as_str()) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            outbound = outbound.header(name.as_str(), v);
        }
    }
    if let Some(key) = &state.api_key {
        // No `ApiType` is available at this layer, so send both header
        // schemes a provider might expect -- harmless, since a real
        // upstream only reads the one it understands.
        outbound = outbound.header("x-api-key", &key.0).bearer_auth(&key.0);
    }
    let upstream_resp = outbound.body(body_bytes).send().await?;

    let status = StatusCode::from_u16(upstream_resp.status().as_u16())?;
    let mut builder = Response::builder().status(status);
    for (name, value) in upstream_resp.headers().iter() {
        if is_forbidden_response_header(name.as_str()) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            builder = builder.header(name, value);
        }
    }

    let body = Body::from_stream(scan_and_forward(
        upstream_resp.bytes_stream(),
        Arc::clone(&state.tracker),
        state.label.clone(),
    ));
    Ok(builder.body(body)?)
}

struct ScanState<S> {
    inner: std::pin::Pin<Box<S>>,
    buffer: String,
    running: UsageStats,
    tracker: Arc<UsageTracker>,
    label: String,
    /// Set once the inner stream has ended or errored, so a stray extra
    /// poll (permitted, if unusual, by the `Stream` contract) doesn't
    /// re-touch a spent inner stream or double-record usage.
    ended: bool,
}

/// Wrap `inner` so that as bytes flow through unchanged to the client, any
/// usage-accounting fields visible in them (streamed SSE/NDJSON events, or
/// -- once the stream ends -- a single buffered JSON body) are recorded into
/// `tracker`. Never fails the forwarded response due to a parse miss.
fn scan_and_forward<S>(
    inner: S,
    tracker: Arc<UsageTracker>,
    label: String,
) -> impl Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + 'static
where
    S: Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
{
    let state = ScanState {
        inner: Box::pin(inner),
        buffer: String::new(),
        running: UsageStats::default(),
        tracker,
        label,
        ended: false,
    };

    stream::unfold(state, |mut st| async move {
        if st.ended {
            return None;
        }
        match st.inner.next().await {
            Some(Ok(chunk)) => {
                if let Ok(text) = std::str::from_utf8(&chunk) {
                    st.buffer.push_str(text);
                }
                scan_buffered_lines(&mut st.buffer, &mut st.running);
                Some((Ok(chunk), st))
            }
            Some(Err(e)) => {
                st.ended = true;
                Some((Err(std::io::Error::other(e)), st))
            }
            None => {
                finalize_leftover(&st.buffer, &mut st.running);
                st.tracker.record(&st.label, st.running);
                None
            }
        }
    })
}

/// Drain every complete line out of `buffer`, feeding each to `scan_line`.
/// Any trailing partial line is left in `buffer` for the next chunk.
fn scan_buffered_lines(buffer: &mut String, running: &mut UsageStats) {
    while let Some(idx) = buffer.find('\n') {
        let line = buffer[..idx].trim_end_matches('\r').to_string();
        buffer.drain(..=idx);
        scan_line(&line, running);
    }
}

/// Recognize one streaming-framing line: an SSE `data:` payload (Anthropic /
/// OpenAI) or a raw NDJSON object (Ollama). Both shapes are attempted by
/// `usage::parse_usage`, so no `ApiType` is needed to pick between them.
fn scan_line(line: &str, running: &mut UsageStats) {
    let trimmed = line.trim();
    let json_str = if let Some(rest) = trimmed.strip_prefix("data:") {
        rest.trim()
    } else if trimmed.starts_with('{') {
        trimmed
    } else {
        return;
    };
    if json_str.is_empty() || json_str == "[DONE]" {
        return;
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str)
        && let Some(delta) = usage::parse_usage(&json)
    {
        running.merge_max(&delta);
    }
}

/// Whatever is left in `buffer` once the response body is exhausted covers
/// the non-streaming case: the entire body is one JSON document.
fn finalize_leftover(buffer: &str, running: &mut UsageStats) {
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed)
        && let Some(delta) = usage::parse_usage(&json)
    {
        running.merge_max(&delta);
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn scan_line_anthropic_sse_data_event() {
        let mut running = UsageStats::default();
        scan_line(
            r#"data: {"type":"message_delta","usage":{"output_tokens":7}}"#,
            &mut running,
        );
        assert_eq!(running.output_tokens, 7);
    }

    #[test]
    fn scan_line_ignores_done_sentinel() {
        let mut running = UsageStats::default();
        scan_line("data: [DONE]", &mut running);
        assert_eq!(running, UsageStats::default());
    }

    #[test]
    fn scan_line_ollama_ndjson_line() {
        let mut running = UsageStats::default();
        scan_line(
            r#"{"done":true,"prompt_eval_count":3,"eval_count":9}"#,
            &mut running,
        );
        assert_eq!(running.input_tokens, 3);
        assert_eq!(running.output_tokens, 9);
    }

    #[test]
    fn finalize_leftover_parses_full_non_streaming_body() {
        let mut running = UsageStats::default();
        finalize_leftover(
            r#"{"usage":{"input_tokens":11,"output_tokens":22}}"#,
            &mut running,
        );
        assert_eq!(running.input_tokens, 11);
        assert_eq!(running.output_tokens, 22);
    }

    #[tokio::test]
    async fn scan_and_forward_records_usage_once_stream_ends() {
        let tracker = Arc::new(UsageTracker::new());
        let chunks: Vec<reqwest::Result<bytes::Bytes>> = vec![
            Ok(bytes::Bytes::from_static(
                b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n",
            )),
            Ok(bytes::Bytes::from_static(
                b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n",
            )),
        ];
        let inner = stream::iter(chunks);
        let forwarded: Vec<_> = scan_and_forward(inner, Arc::clone(&tracker), "chat".to_string())
            .collect()
            .await;
        assert_eq!(forwarded.len(), 2);

        let snapshot = tracker.snapshot();
        let chat = snapshot.get("chat").unwrap();
        assert_eq!(chat.requests, 1);
        assert_eq!(chat.input_tokens, 5);
        assert_eq!(chat.output_tokens, 9);
    }

    #[test]
    fn forbidden_headers_are_filtered_in_both_directions() {
        assert!(is_forbidden_request_header("Authorization"));
        assert!(is_forbidden_request_header("X-Api-Key"));
        assert!(is_forbidden_request_header("Host"));
        assert!(!is_forbidden_request_header("Content-Type"));

        assert!(is_forbidden_response_header("Transfer-Encoding"));
        assert!(!is_forbidden_response_header("Content-Type"));
    }
}
