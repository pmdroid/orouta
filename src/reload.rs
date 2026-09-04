use crate::AppState;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::interval;

const POLL: Duration = Duration::from_secs(1);

pub fn spawn(path: PathBuf, state: AppState) {
    tokio::spawn(async move {
        let mut last = mtime(&path);
        let mut tick = interval(POLL);
        loop {
            tick.tick().await;
            let current = mtime(&path);
            if current.is_none() || current == last {
                continue;
            }
            last = current;
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = %e, "config reload");
                    continue;
                }
            };
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
    if old.upstreams != new.upstreams || old.upstream_order != new.upstream_order {
        state.catalog.reset().await;
    }
    state.config.store(Arc::new(new));
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}
