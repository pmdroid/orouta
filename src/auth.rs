use crate::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde_json::json;
use subtle::ConstantTimeEq;

pub const COOKIE: &str = "orouta_key";

pub async fn require_key(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if path == "/login" || path == "/api/login" || path == "/logo.png" {
        return next.run(request).await;
    }
    let config = state.config.load();
    if config.keys.is_empty() {
        return next.run(request).await;
    }
    if let Some(key) = authorized(&config.keys, request.headers()) {
        crate::keys::stamp(&state, &key);
        next.run(request).await
    } else if wants_html(request.headers()) {
        Redirect::to("/login").into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        )
            .into_response()
    }
}

fn authorized(keys: &[String], headers: &HeaderMap) -> Option<String> {
    let mut presented = Vec::new();
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(token) = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
        {
            presented.push(token.trim());
        }
    }
    if let Some(value) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        presented.push(value.trim());
    }
    if let Some(value) = cookie_value(headers, COOKIE) {
        presented.push(value);
    }
    presented
        .iter()
        .find_map(|p| keys.iter().find(|k| token_eq(p, k)).cloned())
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    let prefix = format!("{name}=");
    raw.split(';').map(|p| p.trim()).find_map(|p| {
        let v = p.strip_prefix(&prefix)?;
        Some(v.split(';').next().unwrap_or(v).trim())
    })
}

fn wants_html(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"))
}

pub(crate) fn token_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

pub(crate) fn cookie_safe(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~'))
}
