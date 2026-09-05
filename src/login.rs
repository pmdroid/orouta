use crate::auth::{cookie_safe, token_eq, COOKIE};
use crate::AppState;
use axum::extract::State;
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct LoginBody {
    key: String,
}

pub async fn api(State(state): State<AppState>, body: Option<Json<LoginBody>>) -> Response {
    let keys = state.config.load().keys.clone();
    let Some(Json(body)) = body else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "key required"})),
        )
            .into_response();
    };
    let presented = body.key.trim();
    let Some(matched) = keys.iter().find(|k| token_eq(presented, k)) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid api key"})),
        )
            .into_response();
    };
    if !cookie_safe(matched) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "configured key cannot be stored in a cookie"})),
        )
            .into_response();
    }
    let cookie = format!("{COOKIE}={matched}; HttpOnly; SameSite=Lax; Path=/");
    ([(header::SET_COOKIE, cookie)], Json(json!({"ok": true}))).into_response()
}
