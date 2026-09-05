mod anthropic;
mod auth;
mod catalog;
mod config;
mod health;
mod list;
mod model;
mod proxy;
mod reload;
mod status;

pub use config::Config;
pub use health::Health;
pub use status::HostStats;

use crate::catalog::Catalog;
use arc_swap::ArcSwap;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<Config>>,
    pub client: reqwest::Client,
    pub catalog: Arc<Catalog>,
    pub stats: Arc<RwLock<HashMap<String, Arc<HostStats>>>>,
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
    };
    if let Some(path) = config_path {
        reload::spawn(path, state.clone());
    }
    Router::new()
        .route("/status", get(status::page))
        .route("/status.json", get(status::json))
        .fallback(proxy::handle)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_key,
        ))
        .with_state(state)
}
