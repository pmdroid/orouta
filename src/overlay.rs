use crate::config::{build_upstream, Config};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Overlay {
    #[serde(default)]
    pub hosts: OverlayHosts,
    #[serde(default)]
    pub keys: OverlayKeys,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OverlayKeys {
    #[serde(default)]
    pub added: Vec<OverlayKey>,
    #[serde(default)]
    pub revoked: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayKey {
    pub label: String,
    pub secret: String,
    pub created: String,
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

pub fn load(config_path: &Path) -> Result<Overlay, String> {
    let path = path_for(config_path);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Overlay::default()),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

pub fn save(config_path: &Path, overlay: &Overlay) -> Result<(), String> {
    let path = path_for(config_path);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(overlay).map_err(|e| format!("overlay: {e}"))?;
    {
        let mut f =
            std::fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("chmod {}: {e}", tmp.display()))?;
        }
        f.write_all(text.as_bytes())
            .map_err(|e| format!("write {}: {e}", tmp.display()))?;
        f.sync_all()
            .map_err(|e| format!("sync {}: {e}", tmp.display()))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", path.display()))
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
    let mut keys: Vec<String> = config
        .raw_keys
        .iter()
        .filter(|k| !overlay.keys.revoked.contains(k))
        .cloned()
        .collect();
    for added in &overlay.keys.added {
        if overlay.keys.revoked.contains(&added.secret) {
            continue;
        }
        if !keys.contains(&added.secret) {
            keys.push(added.secret.clone());
        }
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
        keys,
        raw_keys: config.raw_keys.clone(),
        pull_host: config.pull_host.clone(),
        upstreams,
        upstream_order: order,
    }
}
