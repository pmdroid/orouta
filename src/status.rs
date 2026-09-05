use crate::config::Config;
use crate::tps::round1;
use crate::AppState;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Clone)]
pub struct VramModel {
    pub name: String,
    pub size_vram: u64,
}

#[derive(Clone, Default)]
pub struct VramSnapshot {
    pub loaded_bytes: u64,
    pub models: Vec<VramModel>,
}

#[derive(Default)]
pub struct HostStats {
    reachable: AtomicBool,
    latency_ms: AtomicU64,
    requests_total: AtomicU64,
    errors_total: AtomicU64,
    in_flight: AtomicI64,
    last_error: Mutex<Option<String>>,
    vram: Mutex<Option<VramSnapshot>>,
}

impl HostStats {
    pub fn request_started(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.in_flight.fetch_add(1, Ordering::Relaxed);
    }

    pub fn request_finished(&self, latency: Duration, error: Option<String>) {
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        match error {
            None => {
                self.reachable.store(true, Ordering::Relaxed);
                self.latency_ms
                    .store(latency.as_millis() as u64, Ordering::Relaxed);
            }
            Some(msg) => {
                self.errors_total.fetch_add(1, Ordering::Relaxed);
                self.set_last_error(msg);
            }
        }
    }

    pub fn probe_finished(&self, latency: Duration, error: Option<String>) {
        match error {
            None => {
                self.reachable.store(true, Ordering::Relaxed);
                self.latency_ms
                    .store(latency.as_millis() as u64, Ordering::Relaxed);
            }
            Some(msg) => {
                self.reachable.store(false, Ordering::Relaxed);
                self.set_last_error(msg);
            }
        }
    }

    fn set_last_error(&self, msg: String) {
        *self.last_error.lock().unwrap_or_else(|p| p.into_inner()) = Some(msg);
    }

    pub fn reachable(&self) -> bool {
        self.reachable.load(Ordering::Relaxed)
    }

    pub fn latency_ms(&self) -> u64 {
        self.latency_ms.load(Ordering::Relaxed)
    }

    pub fn requests_total(&self) -> u64 {
        self.requests_total.load(Ordering::Relaxed)
    }

    pub fn errors_total(&self) -> u64 {
        self.errors_total.load(Ordering::Relaxed)
    }

    pub fn in_flight(&self) -> i64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn set_vram(&self, snapshot: VramSnapshot) {
        *self.vram.lock().unwrap_or_else(|p| p.into_inner()) = Some(snapshot);
    }

    pub fn clear_vram(&self) {
        *self.vram.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    pub fn vram(&self) -> Option<VramSnapshot> {
        self.vram.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

fn host_models(by_host: &HashMap<String, Vec<String>>, id: &str) -> Vec<String> {
    by_host.get(id).cloned().unwrap_or_default()
}

pub(crate) fn strip_latest(model: &str) -> &str {
    model.strip_suffix(":latest").unwrap_or(model)
}
pub async fn logo() -> Response {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../docs/favicons/icon-192.png").as_slice(),
    )
        .into_response()
}

pub async fn hosts_payload(state: &AppState, config: &Config) -> Vec<Value> {
    let by_host = state
        .catalog
        .model_names_by_host(config, &state.client)
        .await;
    config
        .upstream_order
        .iter()
        .filter_map(|id| {
            let up = config.upstreams.get(id)?;
            let stats = state.stats_for(id);
            let tps = state.tps.per_host(id);
            Some(json!({
                "id": id,
                "base_url": up.base_url,
                "disabled": up.disabled,
                "api_key_set": up.api_key.is_some(),
                "reachable": stats.reachable(),
                "latency_ms": stats.latency_ms(),
                "models": host_models(&by_host, id),
                "requests_total": stats.requests_total(),
                "errors_total": stats.errors_total(),
                "in_flight": stats.in_flight(),
                "last_error": stats.last_error(),
                "tps": tps
                    .iter()
                    .map(|t| json!({
                        "model": t.model,
                        "avg": round1(t.avg),
                        "last": round1(t.last),
                        "prompt": t.prompt.map(round1),
                        "samples": t.samples,
                    }))
                    .collect::<Vec<_>>(),
                "vram": stats.vram().map(|v| {
                    json!({
                        "loaded_bytes": v.loaded_bytes,
                        "models": v
                            .models
                            .iter()
                            .map(|m| json!({"name": m.name, "size_vram": m.size_vram}))
                            .collect::<Vec<_>>(),
                    })
                }),
            }))
        })
        .collect()
}

pub async fn json(State(state): State<AppState>) -> Response {
    let config = state.config.load();
    state.tailscale.spawn_refresh_if_stale(&state.client);
    let ts = state.tailscale.info();
    let hosts = hosts_payload(&state, &config).await;
    let tailscale = ts.map(|t| {
        json!({
            "self": t.self_dns,
            "tailnet": t.tailnet,
            "online": t.online,
            "serving": t.serving,
            "url": t.url,
        })
    });
    Json(json!({ "hosts": hosts, "tailscale": tailscale })).into_response()
}
