use crate::config::Config;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

pub fn tags(config: &Config) -> Response {
    let mut names: Vec<_> = config.models.keys().cloned().collect();
    names.sort();
    let models: Vec<Value> = names
        .iter()
        .map(|n| json!({"name": n, "model": n}))
        .collect();
    Json(json!({"models": models})).into_response()
}

pub fn openai_models(config: &Config) -> Response {
    let mut names: Vec<_> = config.models.keys().cloned().collect();
    names.sort();
    let data: Vec<Value> = names
        .iter()
        .map(|n| json!({"id": n, "object": "model", "owned_by": "orouta"}))
        .collect();
    Json(json!({"object": "list", "data": data})).into_response()
}

pub fn openai_model(config: &Config, id: &str) -> Response {
    if config.models.contains_key(id) {
        Json(json!({"id": id, "object": "model", "owned_by": "orouta"})).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown model"})),
        )
            .into_response()
    }
}
