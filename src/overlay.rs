use crate::config::{build_upstream, Config};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Overlay {
    #[serde(default)]
    pub hosts: OverlayHosts,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OverlayHosts {
    #[serde(default)]
    pub disabled: Vec<String>,
    #[serde(default)]
    pub removed: Vec<String>,
    #[serde(default)]
    pub added: Vec<OverlayHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayHost {
    pub id: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

pub fn path_for(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("orouta.overlay.json")
}

pub fn load(config_path: &Path) -> Overlay {
    std::fs::read_to_string(path_for(config_path))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(config_path: &Path, overlay: &Overlay) -> Result<(), String> {
    let text = serde_json::to_string_pretty(overlay).map_err(|e| format!("overlay: {e}"))?;
    std::fs::write(path_for(config_path), text).map_err(|e| format!("write overlay: {e}"))
}

pub fn apply(overlay: &Overlay, config: &Config) -> Config {
    let mut upstreams = config.upstreams.clone();
    let mut order = config.upstream_order.clone();
    for id in &overlay.hosts.removed {
        if config.upstreams.contains_key(id) {
            tracing::info!(host = %id, "overlay removed host stays removed");
        }
        upstreams.remove(id);
        order.retain(|x| x != id);
    }
    for added in &overlay.hosts.added {
        if overlay.hosts.removed.contains(&added.id) {
            continue;
        }
        match build_upstream(&added.id, &added.base_url, added.api_key.clone()) {
            Ok(up) => {
                order.retain(|x| x != &added.id);
                order.push(added.id.clone());
                upstreams.insert(added.id.clone(), up);
            }
            Err(e) => tracing::warn!(error = %e, "overlay added host invalid"),
        }
    }
    for (id, up) in upstreams.iter_mut() {
        up.disabled = overlay.hosts.disabled.contains(id);
    }
    Config {
        host: config.host.clone(),
        port: config.port,
        keys: config.keys.clone(),
        pull_host: config.pull_host.clone(),
        upstreams,
        upstream_order: order,
    }
}
