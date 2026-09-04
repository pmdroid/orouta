use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub keys: Vec<String>,
    pub upstreams: HashMap<String, Upstream>,
    pub upstream_order: Vec<String>,
    pub models: HashMap<String, ModelEntry>,
}

#[derive(Debug, Clone)]
pub struct Upstream {
    pub id: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub name: String,
    pub upstream_id: String,
    pub upstream_model: Option<String>,
}

#[derive(Deserialize)]
struct FileConfig {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    auth: FileAuth,
    #[serde(default)]
    upstream: Vec<FileUpstream>,
    #[serde(default)]
    model: Vec<FileModel>,
}

#[derive(Deserialize, Default)]
struct FileAuth {
    #[serde(default)]
    keys: Vec<String>,
}

#[derive(Deserialize)]
struct FileUpstream {
    id: String,
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct FileModel {
    name: String,
    upstream: String,
    #[serde(default)]
    upstream_model: Option<String>,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    11434
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let file: FileConfig = toml::from_str(text).map_err(|e| format!("toml: {e}"))?;
        Self::from_file(file)
    }

    fn from_file(file: FileConfig) -> Result<Self, String> {
        let mut upstreams = HashMap::new();
        let mut upstream_order = Vec::new();
        for u in file.upstream {
            if upstreams.contains_key(&u.id) {
                return Err(format!("duplicate upstream id: {}", u.id));
            }
            let parsed = reqwest::Url::parse(&u.base_url)
                .map_err(|e| format!("upstream {} base_url: {e}", u.id))?;
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err(format!("upstream {} base_url must be http or https", u.id));
            }
            if parsed.host_str().is_none() {
                return Err(format!("upstream {} base_url has no host", u.id));
            }
            let base_url = u.base_url.trim_end_matches('/').to_string();
            let api_key = u.api_key.and_then(|k| {
                let t = k.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            });
            upstream_order.push(u.id.clone());
            upstreams.insert(
                u.id.clone(),
                Upstream {
                    id: u.id,
                    base_url,
                    api_key,
                },
            );
        }
        if upstream_order.is_empty() {
            return Err("at least one [[upstream]] is required".into());
        }
        if file.host.trim().is_empty() {
            return Err("host is required".into());
        }
        let mut models = HashMap::new();
        for m in file.model {
            if models.contains_key(&m.name) {
                return Err(format!("duplicate model name: {}", m.name));
            }
            if !upstreams.contains_key(&m.upstream) {
                return Err(format!(
                    "model {} references unknown upstream {}",
                    m.name, m.upstream
                ));
            }
            let upstream_model = m.upstream_model.and_then(|s| {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            });
            models.insert(
                m.name.clone(),
                ModelEntry {
                    name: m.name,
                    upstream_id: m.upstream,
                    upstream_model,
                },
            );
        }
        Ok(Config {
            host: file.host,
            port: file.port,
            keys: file.auth.keys,
            upstreams,
            upstream_order,
            models,
        })
    }

    pub fn listen_addr(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    pub fn first_upstream(&self) -> &Upstream {
        &self.upstreams[&self.upstream_order[0]]
    }

    pub fn model_upstream(&self, name: &str) -> Option<(&ModelEntry, &Upstream)> {
        let entry = self
            .models
            .get(name)
            .or_else(|| self.models.get(&format!("{name}:latest")))?;
        let up = self.upstreams.get(&entry.upstream_id)?;
        Some((entry, up))
    }
}
