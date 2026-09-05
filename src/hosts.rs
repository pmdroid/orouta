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
    let overlay_path = match guard(&state) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if let Err(e) = build_upstream(&body.id, &body.base_url, body.api_key.clone()) {
        return bad_request(e);
    }
    let _lock = state.overlay_lock.lock().await;
    let mut o = match overlay::load(overlay_path) {
        Ok(o) => o,
        Err(e) => return server_error(e),
    };
    let config = state.config.load().clone();
    if config.upstreams.contains_key(&body.id) {
        return bad_request(format!("duplicate upstream id: {}", body.id));
    }
    o.hosts.removed.retain(|x| x != &body.id);
    o.hosts.added.retain(|h| h.id != body.id);
    o.hosts.added.push(OverlayHost {
        id: body.id,
        base_url: body.base_url,
        api_key: body.api_key,
    });
    if let Err(e) = persist(&state, overlay_path, &o).await {
        return server_error(e);
    }
    list_response(&state).await
}

pub async fn disable(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let overlay_path = match guard(&state) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let _lock = state.overlay_lock.lock().await;
    if !host_exists(&state, &id) {
        return not_found(&id);
    }
    set_disabled(&state, overlay_path, &id, true).await
}

pub async fn enable(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let overlay_path = match guard(&state) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let _lock = state.overlay_lock.lock().await;
    if !host_exists(&state, &id) {
        return not_found(&id);
    }
    set_disabled(&state, overlay_path, &id, false).await
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let overlay_path = match guard(&state) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let _lock = state.overlay_lock.lock().await;
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
    let mut o = match overlay::load(overlay_path) {
        Ok(o) => o,
        Err(e) => return server_error(e),
    };
    if !o.hosts.removed.contains(&id) {
        o.hosts.removed.push(id.clone());
    }
    o.hosts.added.retain(|h| h.id != id);
    o.hosts.disabled.retain(|x| x != &id);
    if let Err(e) = persist(&state, overlay_path, &o).await {
        return server_error(e);
    }
    list_response(&state).await
}

fn guard(state: &AppState) -> Result<&std::path::Path, Box<Response>> {
    let config = state.config.load();
    if config.keys.is_empty() {
        return Err(Box::new(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "host management requires configured auth keys"})),
            )
                .into_response(),
        ));
    }
    match &state.overlay {
        Some(p) => Ok(p),
        None => Err(Box::new(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "host management unavailable"})),
            )
                .into_response(),
        )),
    }
}

fn host_exists(state: &AppState, id: &str) -> bool {
    state.config.load().upstreams.contains_key(id)
}

async fn set_disabled(
    state: &AppState,
    overlay_path: &std::path::Path,
    id: &str,
    disabled: bool,
) -> Response {
    let mut o = match overlay::load(overlay_path) {
        Ok(o) => o,
        Err(e) => return server_error(e),
    };
    if disabled {
        if !o.hosts.disabled.iter().any(|x| x == id) {
            o.hosts.disabled.push(id.to_string());
        }
    } else {
        o.hosts.disabled.retain(|x| x != id);
    }
    if let Err(e) = persist(state, overlay_path, &o).await {
        return server_error(e);
    }
    list_response(state).await
}

async fn persist(
    state: &AppState,
    overlay_path: &std::path::Path,
    o: &Overlay,
) -> Result<(), String> {
    overlay::save(overlay_path, o)?;
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

fn server_error(msg: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": msg})),
    )
        .into_response()
}

fn not_found(id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "unknown host", "host": id})),
    )
        .into_response()
}
