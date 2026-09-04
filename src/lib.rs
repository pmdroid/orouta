mod anthropic;
mod auth;
mod catalog;
mod config;
mod list;
mod model;
mod proxy;

pub use config::Config;

use crate::catalog::Catalog;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::Router;
use std::sync::Arc;

pub const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub client: reqwest::Client,
    pub catalog: Arc<Catalog>,
}

pub fn app(config: Arc<Config>, client: reqwest::Client) -> Router {
    let state = AppState {
        config,
        client,
        catalog: Arc::new(Catalog::new()),
    };
    Router::new()
        .fallback(proxy::handle)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_key,
        ))
        .with_state(state)
}
