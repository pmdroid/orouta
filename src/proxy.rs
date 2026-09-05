use crate::config::Upstream;
use crate::model;
use crate::tps::{self, TpsStore};
use crate::{anthropic, list, AppState, MAX_BODY};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use serde_json::json;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

pub async fn handle(State(state): State<AppState>, req: Request<Body>) -> Response {
    let config = state.config.load();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    let body = match axum::body::to_bytes(req.into_body(), MAX_BODY).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error": "payload too large"})),
            )
                .into_response();
        }
    };
    let path = uri.path();
    if method == Method::POST && path == "/v1/messages" {
        return anthropic::messages(state, body).await;
    }
    if method == Method::GET && path == "/api/tags" {
        return list::tags(&state).await;
    }
    if method == Method::GET && path == "/v1/models" {
        return list::openai_models(&state).await;
    }
    if method == Method::GET {
        if let Some(id) = path.strip_prefix("/v1/models/") {
            if !id.is_empty() {
                return list::openai_model(&state, id).await;
            }
        }
    }
    let pq = uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| path.to_string());
    if path == "/api/copy" {
        return forward_copy(&state, method, &pq, &headers, body).await;
    }
    if method == Method::POST && path == "/api/pull" {
        return pull(&state, &uri, &headers, body).await;
    }
    let name = model::extract_name(&body);
    if let Some(name) = name {
        let capture_model = (method == Method::POST
            && (path == "/api/chat" || path == "/api/generate"))
            .then_some(name.clone());
        if let Some(upstream) = state.catalog.lookup(&config, &state.client, &name).await {
            if let Some(res) = host_down_response(&state, &upstream).await {
                return res;
            }
            return forward(
                &state,
                method,
                &pq,
                &headers,
                body,
                &upstream,
                capture_model.as_deref(),
            )
            .await;
        }
        return unknown_model_response(&state).await;
    }
    match config.first_upstream() {
        Some(up) => forward(&state, method, &pq, &headers, body, up, None).await,
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "no upstreams"})),
        )
            .into_response(),
    }
}

pub async fn host_down_response(state: &AppState, upstream: &Upstream) -> Option<Response> {
    if state.catalog.health.is_up(&upstream.id).await {
        return None;
    }
    let config = state.config.load();
    state.catalog.refresh(&config, &state.client).await;
    if state.catalog.health.is_up(&upstream.id).await {
        return None;
    }
    Some(
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "host unavailable", "host": upstream.id})),
        )
            .into_response(),
    )
}

pub async fn unknown_model_response(state: &AppState) -> Response {
    let config = state.config.load();
    let order: Vec<String> = config
        .upstream_order
        .iter()
        .filter(|id| {
            config
                .upstreams
                .get(*id)
                .map(|u| !u.disabled)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    if let Some(id) = state.catalog.health.first_down(&order).await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "host unavailable", "host": id})),
        )
            .into_response();
    }
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "unknown model"})),
    )
        .into_response()
}

async fn forward_copy(
    state: &AppState,
    method: Method,
    pq: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Response {
    let config = state.config.load();
    let (source, dest) = model::copy_names(&body);
    let su = match &source {
        Some(n) => state
            .catalog
            .lookup(&config, &state.client, n)
            .await
            .map(|u| u.id),
        None => None,
    };
    let du = match &dest {
        Some(n) => state
            .catalog
            .lookup(&config, &state.client, n)
            .await
            .map(|u| u.id),
        None => None,
    };
    let upstream = match (su, du) {
        (Some(a), Some(b)) if a != b => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "source and destination resolve to different upstreams"})),
            )
                .into_response();
        }
        (Some(a), Some(_)) => config.upstreams[&a].clone(),
        (Some(a), None) => config.upstreams[&a].clone(),
        (None, Some(b)) => config.upstreams[&b].clone(),
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "unknown model"})),
            )
                .into_response();
        }
    };
    forward(state, method, pq, headers, body, &upstream, None).await
}

async fn pull(
    state: &AppState,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: Bytes,
) -> Response {
    let host = query_param(uri.query(), "host");
    let config = state.config.load();
    let upstream = match config.resolve_pull_host(host) {
        Ok(u) => u,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
        }
    };
    let path = uri.path();
    let pq = match uri.query() {
        Some(q) => {
            let kept: Vec<&str> = q.split('&').filter(|kv| !kv.starts_with("host=")).collect();
            if kept.is_empty() {
                path.to_string()
            } else {
                format!("{}?{}", path, kept.join("&"))
            }
        }
        None => path.to_string(),
    };
    forward(state, Method::POST, &pq, headers, body, &upstream, None).await
}

fn query_param<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    let q = query?;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then_some(v)
    })
}

pub async fn forward(
    state: &AppState,
    method: Method,
    path_and_query: &str,
    headers: &HeaderMap,
    body: Bytes,
    upstream: &Upstream,
    model: Option<&str>,
) -> Response {
    let url = format!("{}{path_and_query}", upstream.base_url);
    let reqwest_method = match reqwest::Method::from_bytes(method.as_str().as_bytes()) {
        Ok(m) => m,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "method"}))).into_response();
        }
    };
    let mut builder = state.client.request(reqwest_method, url).body(body);
    if let Some(ct) = headers.get(header::CONTENT_TYPE) {
        if let Ok(v) = reqwest::header::HeaderValue::from_bytes(ct.as_bytes()) {
            builder = builder.header(reqwest::header::CONTENT_TYPE, v);
        }
    }
    if let Some(key) = &upstream.api_key {
        builder = builder.header(reqwest::header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let stats = state.stats_for(&upstream.id);
    stats.request_started();
    let start = std::time::Instant::now();
    match builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                stats.request_finished(start.elapsed(), None);
            } else {
                stats.request_finished(start.elapsed(), Some(format!("http {}", status.as_u16())));
            }
            state.catalog.health.record_ok(&upstream.id).await;
            let capture = model.map(|m| Capture {
                store: state.tps.clone(),
                host_id: upstream.id.clone(),
                model: m.to_string(),
                content_type: None,
            });
            pipe_response_with_capture(resp, capture).await
        }
        Err(e) => {
            state
                .catalog
                .health
                .record_error(&upstream.id, e.to_string())
                .await;
            tracing::error!(error = %e, "upstream");
            stats.request_finished(start.elapsed(), Some(e.to_string()));
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "upstream unavailable", "host": &upstream.id})),
            )
                .into_response()
        }
    }
}

pub async fn pipe_response(resp: reqwest::Response) -> Response {
    pipe_response_with_capture(resp, None).await
}

pub async fn pipe_response_with_capture(
    resp: reqwest::Response,
    capture: Option<Capture>,
) -> Response {
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = hop_headers(resp.headers());
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let stream = resp
        .bytes_stream()
        .map(|chunk| chunk.map_err(io::Error::other));
    let body = match capture {
        Some(c) => Body::from_stream(CaptureStream {
            inner: stream,
            tail: Vec::new(),
            capture: Capture { content_type, ..c },
        }),
        None => Body::from_stream(stream),
    };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

pub struct Capture {
    store: Arc<TpsStore>,
    host_id: String,
    model: String,
    content_type: Option<String>,
}

struct CaptureStream<S> {
    inner: S,
    tail: Vec<u8>,
    capture: Capture,
}

impl<S> futures_util::Stream for CaptureStream<S>
where
    S: Stream<Item = Result<Bytes, io::Error>> + Unpin,
{
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                this.tail.extend_from_slice(&chunk);
                if this.tail.len() > tps::TAIL {
                    let excess = this.tail.len() - tps::TAIL;
                    this.tail.drain(..excess);
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(None) => {
                if let Some(fields) = tps::capture(&this.tail, this.capture.content_type.as_deref())
                {
                    this.capture.store.record(
                        &this.capture.host_id,
                        crate::status::strip_latest(&this.capture.model),
                        fields,
                    );
                }
                Poll::Ready(None)
            }
            other => other,
        }
    }
}

fn hop_headers(from: &reqwest::header::HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for name in ["content-type", "cache-control", "x-request-id"] {
        if let Some(v) = from.get(name) {
            if let (Ok(hn), Ok(hv)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_bytes(v.as_bytes()),
            ) {
                out.insert(hn, hv);
            }
        }
    }
    out
}
