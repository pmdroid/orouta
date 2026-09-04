use crate::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::time::interval;

const POLL: Duration = Duration::from_secs(1);

pub fn spawn(path: PathBuf, state: AppState) {
    tokio::spawn(async move {
        let mut last = fs::read_to_string(&path).await.ok();
        let mut tick = interval(POLL);
        loop {
            tick.tick().await;
            let text = match fs::read_to_string(&path).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = %e, "config reload");
                    continue;
                }
            };
            if Some(&text) == last.as_ref() {
                continue;
            }
            last = Some(text.clone());
            match crate::Config::parse(&text) {
                Ok(new) => apply(&state, new).await,
                Err(e) => tracing::error!(error = %e, "config reload"),
            }
        }
    });
}

async fn apply(state: &AppState, new: crate::Config) {
    let old = state.config.load();
    if old.host != new.host || old.port != new.port {
        tracing::warn!(
            old_host = %old.host,
            old_port = old.port,
            new_host = %new.host,
            new_port = new.port,
            "bind host/port change requires restart; keeping current listener"
        );
    }
    let new = match &state.overlay {
        Some(path) => crate::overlay::apply(&crate::overlay::load(path), &new),
        None => new,
    };
    if old.upstreams != new.upstreams || old.upstream_order != new.upstream_order {
        state.catalog.reset().await;
    }
    state.config.store(Arc::new(new));
}
