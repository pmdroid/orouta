use crate::config::Upstream;
use crate::model;
use crate::{anthropic, list, AppState, MAX_BODY};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::json;

pub async fn handle(State(state): State<AppState>, req: Request<Body>) -> Response {
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
    let name = model::extract_name(&body);
    if let Some(name) = name {
        if let Some(upstream) = state
            .catalog
            .lookup(&state.config, &state.client, &name)
            .await
        {
            return forward(&state, method, &pq, &headers, body, &upstream).await;
        }
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown model"})),
        )
            .into_response();
    }
    forward(
        &state,
        method,
        &pq,
        &headers,
        body,
        state.config.first_upstream(),
    )
    .await
}

async fn forward_copy(
    state: &AppState,
    method: Method,
    pq: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Response {
    let (source, dest) = model::copy_names(&body);
    let su = match &source {
        Some(n) => state
            .catalog
            .lookup(&state.config, &state.client, n)
            .await
            .map(|u| u.id),
        None => None,
    };
    let du = match &dest {
        Some(n) => state
            .catalog
            .lookup(&state.config, &state.client, n)
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
        (Some(a), Some(_)) => state.config.upstreams[&a].clone(),
        (Some(a), None) => state.config.upstreams[&a].clone(),
        (None, Some(b)) => state.config.upstreams[&b].clone(),
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "unknown model"})),
            )
                .into_response();
        }
    };
    forward(state, method, pq, headers, body, &upstream).await
}

pub async fn forward(
    state: &AppState,
    method: Method,
    path_and_query: &str,
    headers: &HeaderMap,
    body: Bytes,
    upstream: &Upstream,
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
    let stats = &state.stats[&upstream.id];
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
            pipe_response(resp).await
        }
        Err(e) => {
            tracing::error!(error = %e, "upstream");
            stats.request_finished(start.elapsed(), Some(e.to_string()));
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "upstream unavailable"})),
            )
                .into_response()
        }
    }
}

pub async fn pipe_response(resp: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = hop_headers(resp.headers());
    let stream = resp
        .bytes_stream()
        .map(|chunk| chunk.map_err(std::io::Error::other));
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
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
