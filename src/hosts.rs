use crate::config::build_upstream;
use crate::overlay::{self, Overlay, OverlayHost};
use crate::status;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct AddHost {
    id: String,
    base_url: String,
    api_key: Option<String>,
}

pub async fn add(State(state): State<AppState>, Json(body): Json<AddHost>) -> Response {
    if let Some(r) = guard(&state) {
        return r;
    }
    if let Err(e) = build_upstream(&body.id, &body.base_url, body.api_key.clone()) {
        return bad_request(e);
    }
    let config = state.config.load().clone();
    if config.upstreams.contains_key(&body.id) {
        return bad_request(format!("duplicate upstream id: {}", body.id));
    }
    let mut o = overlay::load(state.overlay.as_ref().unwrap());
    o.hosts.added.push(OverlayHost {
        id: body.id,
        base_url: body.base_url,
        api_key: body.api_key,
    });
    if let Err(e) = persist(&state, &o).await {
        return bad_request(e);
    }
    list_response(&state).await
}

pub async fn disable(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if let Some(r) = guard(&state) {
        return r;
    }
    if !host_exists(&state, &id) {
        return not_found(&id);
    }
    set_disabled(&state, &id, true).await
}

pub async fn enable(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if let Some(r) = guard(&state) {
        return r;
    }
    if !host_exists(&state, &id) {
        return not_found(&id);
    }
    set_disabled(&state, &id, false).await
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if let Some(r) = guard(&state) {
        return r;
    }
    if !host_exists(&state, &id) {
        return not_found(&id);
    }
    if state.stats_for(&id).in_flight() > 0 {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "host has in-flight requests", "host": id})),
        )
            .into_response();
    }
    let mut o = overlay::load(state.overlay.as_ref().unwrap());
    if !o.hosts.removed.contains(&id) {
        o.hosts.removed.push(id.clone());
    }
    o.hosts.added.retain(|h| h.id != id);
    o.hosts.disabled.retain(|x| x != &id);
    if let Err(e) = persist(&state, &o).await {
        return bad_request(e);
    }
    list_response(&state).await
}

fn guard(state: &AppState) -> Option<Response> {
    let config = state.config.load();
    if config.keys.is_empty() {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "host management requires configured auth keys"})),
            )
                .into_response(),
        );
    }
    if state.overlay.is_none() {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "host management unavailable"})),
            )
                .into_response(),
        );
    }
    None
}

fn host_exists(state: &AppState, id: &str) -> bool {
    state.config.load().upstreams.contains_key(id)
}

async fn set_disabled(state: &AppState, id: &str, disabled: bool) -> Response {
    let mut o = overlay::load(state.overlay.as_ref().unwrap());
    if disabled {
        if !o.hosts.disabled.iter().any(|x| x == id) {
            o.hosts.disabled.push(id.to_string());
        }
    } else {
        o.hosts.disabled.retain(|x| x != id);
    }
    if let Err(e) = persist(state, &o).await {
        return bad_request(e);
    }
    list_response(state).await
}

async fn persist(state: &AppState, o: &Overlay) -> Result<(), String> {
    overlay::save(state.overlay.as_ref().unwrap(), o)?;
    let merged = overlay::apply(o, &state.config.load().clone());
    state.config.store(Arc::new(merged));
    state.catalog.reset().await;
    Ok(())
}

async fn list_response(state: &AppState) -> Response {
    let config = state.config.load().clone();
    let hosts: Vec<Value> = status::hosts_payload(state, &config).await;
    Json(json!({ "hosts": hosts })).into_response()
}

fn bad_request(msg: String) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"error": msg}))).into_response()
}

fn not_found(id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "unknown host", "host": id})),
    )
        .into_response()
}
