mod anthropic;
mod auth;
mod catalog;
mod config;
mod health;
mod hosts;
mod keys;
mod list;
mod login;
mod model;
mod overlay;
mod proxy;
mod reload;
mod status;
mod tailscale;
mod tps;

pub use config::Config;
pub use health::Health;
pub use status::HostStats;
pub use tailscale::{Tailscale, TsInfo};

use crate::catalog::Catalog;
use arc_swap::ArcSwap;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

pub const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<Config>>,
    pub client: reqwest::Client,
    pub catalog: Arc<Catalog>,
    pub stats: Arc<RwLock<HashMap<String, Arc<HostStats>>>>,
    pub tps: Arc<tps::TpsStore>,
    pub tailscale: Arc<Tailscale>,
    pub overlay: Option<PathBuf>,
    pub overlay_lock: Arc<tokio::sync::Mutex<()>>,
    pub key_usage: Arc<Mutex<HashMap<String, u64>>>,
}

impl AppState {
    pub fn stats_for(&self, id: &str) -> Arc<HostStats> {
        if let Ok(map) = self.stats.read() {
            if let Some(s) = map.get(id) {
                return s.clone();
            }
        }
        let s = Arc::new(HostStats::default());
        if let Ok(mut map) = self.stats.write() {
            map.entry(id.to_string()).or_insert_with(|| s.clone());
        }
        s
    }
}

pub fn app(config: Arc<Config>, client: reqwest::Client, config_path: Option<PathBuf>) -> Router {
    app_with_tailscale(config, client, config_path, Arc::new(Tailscale::new()))
}

pub fn app_with_tailscale(
    config: Arc<Config>,
    client: reqwest::Client,
    config_path: Option<PathBuf>,
    tailscale: Arc<Tailscale>,
) -> Router {
    let config: Arc<Config> = match &config_path {
        Some(p) => match overlay::load(p) {
            Ok(o) => Arc::new(overlay::apply(&o, &config)),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        None => config,
    };
    let stats: HashMap<String, Arc<HostStats>> = config
        .upstream_order
        .iter()
        .map(|id| (id.clone(), Arc::new(HostStats::default())))
        .collect();
    let stats = Arc::new(RwLock::new(stats));
    let state = AppState {
        config: Arc::new(ArcSwap::from(config)),
        client,
        catalog: Arc::new(Catalog::new(stats.clone())),
        stats,
        tps: Arc::new(tps::TpsStore::new()),
        tailscale,
        overlay: config_path.clone(),
        overlay_lock: Arc::new(tokio::sync::Mutex::new(())),
        key_usage: Arc::new(Mutex::new(HashMap::new())),
    };
    if let Some(path) = state.overlay.clone() {
        reload::spawn(path, state.clone());
    }
    Router::new()
        .route("/status", get(status::page))
        .route("/status.json", get(status::json))
        .route("/logo.png", get(status::logo))
        .route("/keys", get(keys::page))
        .route("/login", get(login::page))
        .route("/api/login", post(login::api))
        .route("/api/keys", post(keys::create))
        .route("/api/keys/{id}", delete(keys::revoke))
        .route("/api/hosts", post(hosts::add))
        .route("/api/hosts/{id}/disable", post(hosts::disable))
        .route("/api/hosts/{id}/enable", post(hosts::enable))
        .route("/api/hosts/{id}", delete(hosts::remove))
        .fallback(proxy::handle)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_key,
        ))
        .with_state(state)
}
