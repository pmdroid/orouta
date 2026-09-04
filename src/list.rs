use crate::AppState;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub async fn tags(state: &AppState) -> Response {
    let body = state
        .catalog
        .tags_body(&state.config, &state.client)
        .await;
    Json(body).into_response()
}

pub async fn openai_models(state: &AppState) -> Response {
    let names = state.catalog.names(&state.config, &state.client).await;
    let data: Vec<_> = names
        .iter()
        .map(|n| json!({"id": n, "object": "model", "owned_by": "orouta"}))
        .collect();
    Json(json!({"object": "list", "data": data})).into_response()
}

pub async fn openai_model(state: &AppState, id: &str) -> Response {
    if state.catalog.has(&state.config, &state.client, id).await {
        Json(json!({"id": id, "object": "model", "owned_by": "orouta"})).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown model"})),
        )
            .into_response()
    }
}
