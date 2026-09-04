use crate::AppState;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use subtle::ConstantTimeEq;

pub async fn require_key(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let config = state.config.load();
    if config.keys.is_empty() {
        return next.run(request).await;
    }
    if authorized(&config.keys, request.headers()) {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        )
            .into_response()
    }
}

fn authorized(keys: &[String], headers: &axum::http::HeaderMap) -> bool {
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
    presented
        .iter()
        .any(|p| keys.iter().any(|k| token_eq(p, k)))
}

fn token_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}
